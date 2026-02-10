// Knot VS Code Extension - LSP Client
//
// This extension provides IDE support for .knot files by connecting to the knot-lsp server.
// Features:
// - Diagnostics (parsing errors, invalid options)
// - Document symbols (chunk navigation)
// - Hover information
// - Completion suggestions

import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import { workspace, ExtensionContext, window, commands, Uri } from 'vscode';
import { ChildProcess, spawn } from 'child_process';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
    ExecuteCommandRequest,
} from 'vscode-languageclient/node';
import { KnotProjectProvider } from './projectExplorer';
import { resolveBinaryPath, findProjectRoot } from './utils';

let client: LanguageClient | undefined;
let watchProcesses: Map<string, ChildProcess> = new Map();

export async function activate(context: ExtensionContext) {
    const outputChannel = window.createOutputChannel('Knot Extension');
    outputChannel.appendLine('Activating Knot extension...');

    // Register Knot Project View
    const knotProjectProvider = new KnotProjectProvider();
    window.registerTreeDataProvider('knotExplorer', knotProjectProvider);
    
    context.subscriptions.push(
        commands.registerCommand('knot.refreshProjectView', () => knotProjectProvider.refresh())
    );

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

    // Air path resolution (kept for future manual use)
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

    // Register commands
    context.subscriptions.push(
        commands.registerCommand('knot.openPreview', async () => {
            await openPreview(outputChannel);
        })
    );

    context.subscriptions.push(
        commands.registerCommand('knot.stopWatch', async () => {
            const editor = window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'knot') {
                return;
            }

            const knotPath = editor.document.uri.fsPath;
            const projectRoot = findProjectRoot(path.dirname(knotPath));
            if (!projectRoot) {
                window.showErrorMessage('Could not find project root');
                return;
            }

            if (watchProcesses.has(projectRoot)) {
                outputChannel.appendLine(`Stopping knot watch for ${projectRoot}...`);
                const process = watchProcesses.get(projectRoot);
                if (process) {
                    process.kill();
                    watchProcesses.delete(projectRoot);
                    window.showInformationMessage(`Knot preview stopped for ${path.basename(projectRoot)}`);
                }
            } else {
                window.showInformationMessage('No active Knot preview running for this project');
            }
        })
    );

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
    for (const [knotPath, process] of watchProcesses) {
        process.kill();
    }
    watchProcesses.clear();
}

async function openPreview(outputChannel: any): Promise<void> {
    const editor = window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'knot') { return; }

    const knotPath = editor.document.uri.fsPath;
    const knotDir = path.dirname(knotPath);
    const projectRoot = findProjectRoot(knotDir);
    if (!projectRoot) {
        window.showErrorMessage('Could not find knot.toml in parent directories');
        return;
    }

    const projectName = path.basename(projectRoot);
    const pdfPath = path.join(projectRoot, `${projectName}.pdf`);

    if (!watchProcesses.has(projectRoot)) {
        outputChannel.appendLine('Starting knot watch...');
        const knotBinary = resolveBinaryPath('knot', outputChannel);
        try {
            const watchProcess = spawn(knotBinary, ['watch'], {
                cwd: projectRoot,
                stdio: ['ignore', 'pipe', 'pipe']
            });
            watchProcess.stdout?.on('data', (data) => outputChannel.appendLine(`[knot watch] ${data.toString().trim()}`));
            watchProcess.stderr?.on('data', (data) => outputChannel.appendLine(`[knot watch stderr] ${data.toString().trim()}`));
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