// Knot VS Code Extension - LSP Client
//
// This extension provides IDE support for .knot files by connecting to the knot-lsp server.
// Features:
// - R code formatting with Air (on save)
// - Diagnostics (parsing errors, invalid options)
// - Document symbols (chunk navigation)
// - Hover information
// - Completion suggestions

import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import { workspace, ExtensionContext, window, commands, Uri, WorkspaceEdit, Range, Position } from 'vscode';
import { ChildProcess, spawn } from 'child_process';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
    ExecuteCommandRequest,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let watchProcesses: Map<string, ChildProcess> = new Map();

/**
 * Resolve a binary path by looking in common locations (bin, .cargo/bin, workspace)
 */
function resolveBinaryPath(name: string, outputChannel: any): string {
    const homeBin = path.join(os.homedir(), 'bin', name);
    const cargoBin = path.join(os.homedir(), '.cargo', 'bin', name);
    
    let workspaceBin: string | undefined;
    if (workspace.workspaceFolders && workspace.workspaceFolders.length > 0) {
        workspaceBin = path.join(workspace.workspaceFolders[0].uri.fsPath, 'target', 'release', name);
    }

    if (fs.existsSync(homeBin)) {
        outputChannel.appendLine(`Found ${name} in ~/bin: ${homeBin}`);
        return homeBin;
    } else if (fs.existsSync(cargoBin)) {
        outputChannel.appendLine(`Found ${name} in ~/.cargo/bin: ${cargoBin}`);
        return cargoBin;
    } else if (workspaceBin && fs.existsSync(workspaceBin)) {
        outputChannel.appendLine(`Found ${name} in workspace target/release: ${workspaceBin}`);
        return workspaceBin;
    } else {
        outputChannel.appendLine(`${name} not found in common locations, relying on system PATH`);
        return name;
    }
}

export async function activate(context: ExtensionContext) {
    const outputChannel = window.createOutputChannel('Knot Extension');
    outputChannel.appendLine('Activating Knot extension...');

    const config = workspace.getConfiguration('knot');
    const lspEnabled = config.get<boolean>('lsp.enabled', true);

    if (!lspEnabled) {
        outputChannel.appendLine('Knot LSP is disabled in settings');
        return;
    }

    let lspPath = config.get<string>('lsp.path', 'knot-lsp');
    if (lspPath === 'knot-lsp') {
        lspPath = resolveBinaryPath('knot-lsp', outputChannel);
    }

    const serverOptions: ServerOptions = {
        command: lspPath,
        args: [],
        transport: TransportKind.stdio,
    };

    let airPath = config.get<string>('formatter.air.path', 'air');
    if (airPath === 'air') {
        const homeAir = path.join(os.homedir(), 'bin', 'air');
        if (fs.existsSync(homeAir)) {
            airPath = homeAir;
        } else {
            const extensionsDir = path.join(os.homedir(), '.vscode', 'extensions');
            if (fs.existsSync(extensionsDir)) {
                const dirs = fs.readdirSync(extensionsDir);
                const airDir = dirs.find(d => d.startsWith('posit.air-'));
                if (airDir) {
                    const p1 = path.join(extensionsDir, airDir, 'bin', 'air');
                    const p2 = path.join(extensionsDir, airDir, 'bundled', 'bin', 'air');
                    if (fs.existsSync(p2)) {
                        airPath = p2;
                    } else if (fs.existsSync(p1)) {
                        airPath = p1;
                    }
                }
            }
        }
    }

    let tinymistPath: string | undefined;
    const homeTinymist = path.join(os.homedir(), 'bin', 'tinymist');
    if (fs.existsSync(homeTinymist)) {
        tinymistPath = homeTinymist;
    } else {
        const extensionsDir = path.join(os.homedir(), '.vscode', 'extensions');
        if (fs.existsSync(extensionsDir)) {
            const dirs = fs.readdirSync(extensionsDir);
            const tinymistDir = dirs.find(d => d.startsWith('myriad-dreamin.tinymist-'));
            if (tinymistDir) {
                const candidate = path.join(extensionsDir, tinymistDir, 'out', 'tinymist');
                if (fs.existsSync(candidate)) {
                    tinymistPath = candidate;
                }
            }
        }
    }

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'knot' }],
        synchronize: {
            configurationSection: 'knot',
            fileEvents: workspace.createFileSystemWatcher('**/*.knot'),
        },
        initializationOptions: {
            airPath: airPath,
            tinymistPath: tinymistPath
        },
        outputChannel: outputChannel,
    };

    client = new LanguageClient(
        'knotLanguageServer',
        'Knot Language Server',
        serverOptions,
        clientOptions
    );

    try {
        await client.start();
        outputChannel.appendLine('Knot LSP client started successfully');
    } catch (error) {
        outputChannel.appendLine(`Failed to start Knot LSP client: ${error}`);
    }

    // Register preview command
    context.subscriptions.push(
        commands.registerCommand('knot.openPreview', async () => {
            await openPreview(outputChannel);
        })
    );

    // Format on Save logic
    context.subscriptions.push(
        workspace.onDidSaveTextDocument(async (document) => {
            if (document.languageId !== 'knot' || !client) { return; }
            
            const config = workspace.getConfiguration('knot');
            if (!config.get<boolean>('formatter.formatOnSave', true)) { return; }

            outputChannel.appendLine(`Formatting ${document.uri.toString()} on save...`);
            
            try {
                const edits = await client.sendRequest(ExecuteCommandRequest.type, {
                    command: 'knot.format',
                    arguments: [document.uri.toString()]
                }) as any[];

                if (edits && edits.length > 0) {
                    const workspaceEdit = new WorkspaceEdit();
                    for (const edit of edits) {
                        workspaceEdit.replace(
                            document.uri,
                            new Range(
                                new Position(edit.range.start.line, edit.range.start.character),
                                new Position(edit.range.end.line, edit.range.end.character)
                            ),
                            edit.new_text
                        );
                    }
                    await workspace.applyEdit(workspaceEdit);
                    // Save again after formatting
                    await document.save();
                }
            } catch (error) {
                outputChannel.appendLine(`Formatting error: ${error}`);
            }
        })
    );

    // Register clean project command
    context.subscriptions.push(
        commands.registerCommand('knot.cleanProject', async (resource?: Uri) => {
            if (!client) return;
            let targetUri: string | undefined;
            if (resource) {
                targetUri = resource.toString();
            } else if (window.activeTextEditor && window.activeTextEditor.document.languageId === 'knot') {
                targetUri = window.activeTextEditor.document.uri.toString();
            }
            if (!targetUri) return;

            try {
                await client.sendRequest(ExecuteCommandRequest.type, {
                    command: 'knot.cleanProject',
                    arguments: [targetUri]
                });
            } catch (error) {
                outputChannel.appendLine(`Error during clean: ${error}`);
            }
        })
    );
}

export async function deactivate(): Promise<void> {
    if (client) { await client.stop(); }
    for (const [knotPath, process] of watchProcesses) { process.kill(); }
    watchProcesses.clear();
}

async function openPreview(outputChannel: any): Promise<void> {
    const editor = window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'knot') { return; }

    const knotPath = editor.document.uri.fsPath;
    const knotDir = path.dirname(knotPath);
    const projectRoot = findProjectRoot(knotDir);
    if (!projectRoot) { return; }

    const projectName = path.basename(projectRoot);
    const pdfPath = path.join(projectRoot, `${projectName}.pdf`);

    if (!watchProcesses.has(projectRoot)) {
        const knotBinary = resolveBinaryPath('knot', outputChannel);
        try {
            const watchProcess = spawn(knotBinary, ['watch'], {
                cwd: projectRoot,
                stdio: ['ignore', 'pipe', 'pipe']
            });
            watchProcess.stdout?.on('data', (data) => outputChannel.appendLine(`[knot watch] ${data.toString().trim()}`));
            watchProcess.stderr?.on('data', (data) => outputChannel.appendLine(`[knot watch error] ${data.toString().trim()}`));
            watchProcess.on('exit', () => watchProcesses.delete(projectRoot));
            watchProcesses.set(projectRoot, watchProcess);
            await new Promise(resolve => setTimeout(resolve, 1000));
        } catch (error) {
            outputChannel.appendLine(`Error starting knot watch: ${error}`);
            return;
        }
    }

    const maxWaitTime = 10000;
    const startTime = Date.now();
    while (!fs.existsSync(pdfPath)) {
        if (Date.now() - startTime > maxWaitTime) { return; }
        await new Promise(resolve => setTimeout(resolve, 200));
    }

    const pdfUri = Uri.file(pdfPath);
    await commands.executeCommand('vscode.open', pdfUri, { viewColumn: 2 });
}

function findProjectRoot(startDir: string): string | null {
    let currentDir = startDir;
    while (currentDir !== path.dirname(currentDir)) {
        if (fs.existsSync(path.join(currentDir, 'knot.toml'))) return currentDir;
        currentDir = path.dirname(currentDir);
    }
    return fs.existsSync(path.join(currentDir, 'knot.toml')) ? currentDir : null;
}
