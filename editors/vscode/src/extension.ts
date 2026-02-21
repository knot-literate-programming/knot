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
import { workspace, ExtensionContext, window, commands, Uri, ProgressLocation, StatusBarAlignment, StatusBarItem, Range, Position, ViewColumn, Selection } from 'vscode';
import { ChildProcess, spawn } from 'child_process';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
    ExecuteCommandRequest,
} from 'vscode-languageclient/node';
import { KnotProjectProvider } from './projectExplorer';
import { resolveBinaryPath, findProjectRoot, parseMainFromToml, runKnotCommand, isKnotCompiledTyp } from './utils';

let client: LanguageClient | undefined;
let watchProcesses: Map<string, ChildProcess> = new Map();
let compilationStatusBar: StatusBarItem;
let suppressAutoSync = false;
let syncDebounceTimer: ReturnType<typeof setTimeout> | undefined;

export async function activate(context: ExtensionContext) {
    const outputChannel = window.createOutputChannel('Knot Extension');
    outputChannel.appendLine('Activating Knot extension...');

    // Create status bar item for compilation feedback
    compilationStatusBar = window.createStatusBarItem(StatusBarAlignment.Left, 100);
    context.subscriptions.push(compilationStatusBar);

    // Manual backward sync: jump from current .typ position to the .knot source
    context.subscriptions.push(
        commands.registerCommand('knot.jumpToKnot', async () => {
            const editor = window.activeTextEditor;
            if (!editor || !editor.document.fileName.endsWith('.typ')) {
                window.showInformationMessage('This command must be run from a .typ file');
                return;
            }
            await jumpToKnotSource(outputChannel);
        })
    );

    // Manual forward sync: jump from current .knot position to the .typ compiled file
    context.subscriptions.push(
        commands.registerCommand('knot.jumpToTyp', async () => {
            const editor = window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'knot') {
                window.showInformationMessage('This command must be run from a .knot file');
                return;
            }

            const knotFilePath = editor.document.uri.fsPath;
            const knotLine = editor.selection.active.line;
            const projectRoot = findProjectRoot(path.dirname(knotFilePath));
            if (!projectRoot) {
                window.showErrorMessage('Could not find project root (knot.toml)');
                return;
            }

            const tomlPath = path.join(projectRoot, 'knot.toml');
            const mainFile = parseMainFromToml(tomlPath);
            const mainStem = path.basename(mainFile, path.extname(mainFile));
            const mainTypPath = path.join(projectRoot, `${mainStem}.typ`);
            
            if (!fs.existsSync(mainTypPath)) {
                window.showErrorMessage(`Compiled file not found: ${mainTypPath}. Please run Build or Open Preview first.`);
                return;
            }

            const knotRelFile = path.relative(projectRoot, knotFilePath);
            try {
                const knotBinary = resolveBinaryPath('knot', outputChannel);
                const result = await runKnotCommand(knotBinary, ['jump-to-typ', mainTypPath, knotRelFile, (knotLine + 1).toString()], outputChannel);
                const mappedTypLine = parseInt(result, 10) - 1;

                const typUri = Uri.file(mainTypPath);
                const typDoc = await workspace.openTextDocument(typUri);
                const pos = new Position(mappedTypLine, 0);
                await window.showTextDocument(typDoc, {
                    selection: new Range(pos, pos),
                    viewColumn: ViewColumn.Two,
                    preserveFocus: false,
                });
            } catch (e) {
                window.showErrorMessage(`Jump to Typ failed: ${e}`);
            }
        })
    );

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

    // Air path resolution: honour setting, then common install locations
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

    // Ruff path resolution: honour setting, then ~/bin/ruff
    let ruffPath = config.get<string>('formatter.ruff.path', 'ruff');
    if (ruffPath === 'ruff') {
        const homeRuff = path.join(os.homedir(), 'bin', 'ruff');
        if (fs.existsSync(homeRuff)) {
            ruffPath = homeRuff;
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
            ruffPath: ruffPath,
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

    // Register commands immediately (before LSP start, which can be slow)
    context.subscriptions.push(
        commands.registerCommand('knot.openPreview', async () => {
            await openPreview(outputChannel);
        })
    );

    // Disable auto backward sync for now to avoid focus issues
    /*
    context.subscriptions.push(
        window.onDidChangeTextEditorSelection((event) => {
            // ... (disabled)
        })
    );
    */

    // Forward sync command: scroll preview to current .knot position
    context.subscriptions.push(
        commands.registerCommand('knot.syncPreview', async () => {
            const editor = window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'knot') { return; }

            const knotFilePath = editor.document.uri.fsPath;
            const knotLine = editor.selection.active.line;
            const projectRoot = findProjectRoot(path.dirname(knotFilePath));
            if (!projectRoot) { return; }

            const tomlPath = path.join(projectRoot, 'knot.toml');
            const mainFile = parseMainFromToml(tomlPath);
            const mainStem = path.basename(mainFile, path.extname(mainFile));
            const mainTypPath = path.join(projectRoot, `${mainStem}.typ`);
            if (!fs.existsSync(mainTypPath)) { return; }

            const knotRelFile = path.relative(projectRoot, knotFilePath);
            let mappedTypLine: number;
            try {
                const knotBinary = resolveBinaryPath('knot', outputChannel);
                const result = await runKnotCommand(knotBinary, ['jump-to-typ', mainTypPath, knotRelFile, (knotLine + 1).toString()], outputChannel);
                mappedTypLine = parseInt(result, 10) - 1;
            } catch { return; }
            
            suppressAutoSync = true;
            try {
                const mainTypUri = Uri.file(mainTypPath);
                const mainTypDoc = await workspace.openTextDocument(mainTypUri);
                // 1. Show main.typ to make sure Tinymist knows what to preview
                await window.showTextDocument(mainTypDoc, {
                    viewColumn: ViewColumn.One,
                    preserveFocus: false,
                });
                
                // 2. Set cursor in main.typ
                const mainTypEditor = window.activeTextEditor;
                if (mainTypEditor && mainTypEditor.document.uri.fsPath === mainTypPath) {
                    mainTypEditor.selection = new Selection(
                        new Position(mappedTypLine, 0),
                        new Position(mappedTypLine, 0),
                    );
                }

                // 3. Re-run preview command to be sure panel is focused on main.typ
                await commands.executeCommand('typst-preview.preview');
                
                // 4. Brief pause for the panel to attach
                await new Promise(r => setTimeout(r, 250));
                
                // 5. Call sync (scrolling)
                await commands.executeCommand('typst-preview.sync');
                
                outputChannel.appendLine(`[forward-sync] sync called for main.typ line ${mappedTypLine + 1}`);
            } catch (e) {
                outputChannel.appendLine(`[forward-sync] error: ${e}`);
            } finally {
                suppressAutoSync = false;
            }
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

    context.subscriptions.push(
        commands.registerCommand('knot.formatChunk', async () => {
            if (!client) {
                window.showErrorMessage('Knot LSP client not ready');
                return;
            }
            const editor = window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'knot') return;

            const uri = editor.document.uri.toString();
            const position = editor.selection.active;

            outputChannel.appendLine(`Extension: Triggering formatChunk at line ${position.line}`);

            try {
                // Send custom request to bypass VS Code command interception
                const result = await client.sendRequest('knot/formatChunk', {
                    uri: uri,
                    position: {
                        line: position.line,
                        character: position.character
                    }
                });
                
                outputChannel.appendLine(`Extension: Format result: ${JSON.stringify(result)}`);
            } catch (error) {
                outputChannel.appendLine(`Extension Error: ${error}`);
                window.showErrorMessage(`Format Chunk failed: ${error}`);
            }
        })
    );

    // Start LSP after registering commands (fire-and-forget to avoid blocking activation)
    client.start().then(() => {
        outputChannel.appendLine('Knot LSP client started successfully');
    }).catch((error: unknown) => {
        outputChannel.appendLine(`Failed to start Knot LSP client: ${error}`);
    });
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

    // Read main file from knot.toml and extract stem (e.g., "main.knot" -> "main")
    const tomlPath = path.join(projectRoot, 'knot.toml');
    const mainFile = parseMainFromToml(tomlPath);
    const mainStem = path.basename(mainFile, path.extname(mainFile));
    const pdfPath = path.join(projectRoot, `${mainStem}.pdf`);

    // Show status bar indicator (more visible than notification)
    outputChannel.appendLine(`DEBUG: Starting compilation for ${mainStem}.pdf`);
    compilationStatusBar.text = '$(sync~spin) Compiling Knot...';
    compilationStatusBar.show();
    outputChannel.appendLine('DEBUG: Status bar shown');

    try {
        // Also show progress notification
        await window.withProgress(
            {
                location: ProgressLocation.Notification,
                title: 'Compiling Knot document...',
                cancellable: false
            },
            async (progress) => {
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
                        compilationStatusBar.hide();
                        return;
                    }
                }

                progress.report({ message: 'Waiting for PDF generation...' });
                compilationStatusBar.text = '$(sync~spin) Waiting for PDF...';

                const maxWaitTime = 10000;
                const startTime = Date.now();
                while (!fs.existsSync(pdfPath)) {
                    if (Date.now() - startTime > maxWaitTime) {
                        compilationStatusBar.hide();
                        return;
                    }
                    await new Promise(resolve => setTimeout(resolve, 200));
                }

                                                progress.report({ message: 'Opening PDF...' });
                                                compilationStatusBar.text = '$(check) Compilation complete!';
                                
                                                if (fs.existsSync(pdfPath)) {
                                                    const pdfUri = Uri.file(pdfPath);
                                                    await commands.executeCommand('vscode.open', pdfUri, { viewColumn: ViewColumn.Two });
                                                } else {
                                                    window.showErrorMessage(`PDF not found: ${pdfPath}`);
                                                }
                                
                                                // Hide status bar after a short delay
                                                setTimeout(() => {
                                                    compilationStatusBar.hide();
                                                }, 2000);
                                            }
                                        );
                                    } catch (error) {
                                        outputChannel.appendLine(`ERROR in openPreview: ${error}`);
                                        compilationStatusBar.hide();
                                        throw error;
                                    }
                                }
                                                /**
                 * Manual backward sync command: jump from current .typ to .knot
                 */
                async function jumpToKnotSource(outputChannel: any): Promise<void> {
                    const editor = window.activeTextEditor;
                    if (!editor || !editor.document.fileName.endsWith('.typ')) { return; }
                
                    const doc = editor.document;
                    if (!isKnotCompiledTyp(doc.fileName)) { return; }
                
                    const typLine = editor.selection.active.line;
                
                    try {
                        const knotBinary = resolveBinaryPath('knot', outputChannel);
                        const result = await runKnotCommand(knotBinary, ['jump-to-source', doc.fileName, (typLine + 1).toString()], outputChannel);
                        
                        if (!result || !result.includes(':')) { return; }
                
                        const [knotFile, lineStr] = result.split(':');
                        const knotLine = parseInt(lineStr, 10) - 1;
                
                        if (!fs.existsSync(knotFile)) { return; }
                
                        const knotUri = Uri.file(knotFile);
                        const pos = new Position(knotLine, 0);
                        const knotDoc = await workspace.openTextDocument(knotUri);
                        await window.showTextDocument(knotDoc, {
                            selection: new Range(pos, pos),
                            viewColumn: ViewColumn.One,
                            preserveFocus: false,
                        });
                    } catch { /* ignore */ }
                }
                