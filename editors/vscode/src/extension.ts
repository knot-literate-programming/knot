// Knot VS Code Extension - LSP Client
import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import {
    workspace,
    ExtensionContext,
    window,
    commands,
    env,
    Uri,
    ProgressLocation,
    StatusBarAlignment,
    StatusBarItem,
    Range,
    Position,
    ViewColumn,
    UriHandler,
    Selection,
    TextEditorRevealType,
} from 'vscode';
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
let compilationStatusBar: StatusBarItem;
let suppressAutoSync = false;
let syncDebounceTimer: ReturnType<typeof setTimeout> | undefined;
let forwardSyncTimer: ReturnType<typeof setTimeout> | undefined;

/**
 * Handles URIs in the form of vscode://knot-dev.knot/jump?file=...&line=...
 */
class KnotUriHandler implements UriHandler {
    constructor(private outputChannel: any) {}

    async handleUri(uri: Uri) {
        if (uri.path === '/jump') {
            const query = new URLSearchParams(uri.query);
            const file = query.get('file');
            const line = query.get('line');

            if (file && line) {
                this.outputChannel.appendLine(`[URI Handler] Jump request for ${file}:${line}`);
                try {
                    const knotBinary = resolveBinaryPath('knot', this.outputChannel);
                    const result = await runKnotCommand(knotBinary, ['jump-to-source', file, line], this.outputChannel);
                    
                    if (result && result.includes(':')) {
                        const [knotFile, lineStr] = result.split(':');
                        const knotLine = parseInt(lineStr, 10) - 1;
                        const knotUri = Uri.file(knotFile);
                        const targetDoc = await workspace.openTextDocument(knotUri);
                        await window.showTextDocument(targetDoc, {
                            selection: new Range(new Position(knotLine, 0), new Position(knotLine, 0)),
                            viewColumn: ViewColumn.One
                        });
                    }
                } catch (e) {
                    this.outputChannel.appendLine(`[URI Handler] Mapping failed: ${e}`);
                }
            }
        }
    }
}

export async function activate(context: ExtensionContext) {
    const outputChannel = window.createOutputChannel('Knot Extension');
    outputChannel.appendLine('Activating Knot extension...');

    // Register URI handler for clean backward sync
    context.subscriptions.push(
        window.registerUriHandler(new KnotUriHandler(outputChannel))
    );

    // Register status bar item
    compilationStatusBar = window.createStatusBarItem(StatusBarAlignment.Left, 100);
    context.subscriptions.push(compilationStatusBar);

    // Auto backward sync: when Tinymist opens a .typ file, redirect immediately to .knot
    context.subscriptions.push(
        window.onDidChangeTextEditorSelection(async (event) => {
            const doc = event.textEditor.document;
            if (!doc.fileName.endsWith('.typ') || doc.fileName.endsWith('.knot.typ')) return;
            if (event.selections.length !== 1 || !event.selections[0].isEmpty) return;
            if (suppressAutoSync) return;
            if (!isKnotCompiledTyp(doc.fileName)) return;

            const typFileName = doc.fileName;
            const typLine = event.selections[0].active.line;

            if (syncDebounceTimer) clearTimeout(syncDebounceTimer);
            syncDebounceTimer = setTimeout(async () => {
                syncDebounceTimer = undefined;
                try {
                    const knotBinary = resolveBinaryPath('knot', outputChannel);
                    const result = await runKnotCommand(knotBinary, ['jump-to-source', typFileName, (typLine + 1).toString()], outputChannel);
                    
                    if (result && result.includes(':')) {
                        const [knotFile, lineStr] = result.split(':');
                        const knotLine = parseInt(lineStr, 10) - 1;
                        
                        if (window.activeTextEditor?.document.fileName === typFileName) {
                            await commands.executeCommand('workbench.action.closeActiveEditor');
                        }

                        const knotUri = Uri.file(knotFile);
                        const pos = new Position(knotLine, 0);
                        const targetDoc = await workspace.openTextDocument(knotUri);
                        
                        suppressAutoSync = true;
                        await window.showTextDocument(targetDoc, {
                            selection: new Range(pos, pos),
                            viewColumn: ViewColumn.One,
                            preserveFocus: false,
                        });
                        setTimeout(() => { suppressAutoSync = false; }, 500);
                    }
                } catch (e) {
                    outputChannel.appendLine(`[auto-sync] Error: ${e}`);
                }
            }, 50);
        })
    );

    // Auto forward sync: when cursor moves in a .knot file, scroll Tinymist preview.
    context.subscriptions.push(
        window.onDidChangeTextEditorSelection((event) => {
            const doc = event.textEditor.document;
            if (doc.languageId !== 'knot') return;
            if (!client) return;
            if (event.selections.length !== 1 || !event.selections[0].isEmpty) return;

            const pos = event.selections[0].active;
            const uri = doc.uri.toString();

            if (forwardSyncTimer) clearTimeout(forwardSyncTimer);
            forwardSyncTimer = setTimeout(async () => {
                forwardSyncTimer = undefined;
                try {
                    // knot/syncForward maps the knot line to the corresponding typ line
                    // and scrolls the preview by calling tinymist.scrollPreview on our
                    // tinymist subprocess. Fire-and-forget — result is not used here.
                    await client!.sendRequest('knot/syncForward', {
                        uri,
                        line: pos.line,
                        character: pos.character,
                    });
                } catch (_) {
                    // Silently ignore — knot-lsp may not be ready yet.
                }
            }, 150);
        })
    );

    // Manual jump commands
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
            if (!projectRoot) { return; }

            const tomlPath = path.join(projectRoot, 'knot.toml');
            const mainFile = parseMainFromToml(tomlPath);
            const mainStem = path.basename(mainFile, path.extname(mainFile));
            const mainTypPath = path.join(projectRoot, `${mainStem}.typ`);
            
            if (!fs.existsSync(mainTypPath)) {
                window.showErrorMessage(`Compiled file not found: ${mainTypPath}`);
                return;
            }

            const knotRelFile = path.relative(projectRoot, knotFilePath);
            try {
                const knotBinary = resolveBinaryPath('knot', outputChannel);
                const result = await runKnotCommand(knotBinary, ['jump-to-typ', mainTypPath, knotRelFile, (knotLine + 1).toString()], outputChannel);
                const mappedTypLine = parseInt(result, 10) - 1;

                const typUri = Uri.file(mainTypPath);
                const typDoc = await workspace.openTextDocument(typUri);
                await window.showTextDocument(typDoc, {
                    selection: new Range(new Position(mappedTypLine, 0), new Position(mappedTypLine, 0)),
                    viewColumn: ViewColumn.Active
                });
            } catch (e) {
                window.showErrorMessage(`Jump to Typ failed: ${e}`);
            }
        })
    );

    // Knot Project View
    const knotProjectProvider = new KnotProjectProvider();
    window.registerTreeDataProvider('knotExplorer', knotProjectProvider);
    context.subscriptions.push(
        commands.registerCommand('knot.refreshProjectView', () => knotProjectProvider.refresh())
    );

    const config = workspace.getConfiguration('knot');
    const lspEnabled = config.get<boolean>('lsp.enabled', true);

    if (lspEnabled) {
        let lspPath = config.get<string>('lsp.path', 'knot-lsp');
        if (lspPath === 'knot-lsp') {
            lspPath = resolveBinaryPath('knot-lsp', outputChannel);
        }

        const serverOptions: ServerOptions = { command: lspPath, args: [], transport: TransportKind.stdio };
        const clientOptions: LanguageClientOptions = {
            documentSelector: [{ scheme: 'file', language: 'knot' }],
            synchronize: { configurationSection: 'knot', fileEvents: workspace.createFileSystemWatcher('**/*.knot') },
            outputChannel: outputChannel,
        };

        client = new LanguageClient('knotLanguageServer', 'Knot Language Server', serverOptions, clientOptions);
        client.start();
    }

    context.subscriptions.push(
        commands.registerCommand('knot.openPreview', () => openPreview(outputChannel))
    );

    context.subscriptions.push(
        commands.registerCommand('knot.buildProject', () => buildProject(outputChannel))
    );

    context.subscriptions.push(
        commands.registerCommand('knot.runDocument', async () => {
            const editor = window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'knot') return;
            if (!client) {
                window.showErrorMessage('LSP not connected — cannot run document');
                return;
            }
            if (editor.document.isDirty) {
                // Saving triggers did_save → do_compile automatically.
                await editor.document.save();
            } else {
                // Document already saved: explicitly trigger a fresh compile
                // (e.g. user wants to force re-execution of all chunks).
                await client.sendRequest('knot/compile', {
                    uri: editor.document.uri.toString(),
                });
            }
        })
    );

    context.subscriptions.push(
        commands.registerCommand('knot.cleanProject', async (resource?: Uri) => {
            if (!client) return;
            const targetUri = resource?.toString() || window.activeTextEditor?.document.uri.toString();
            if (targetUri) {
                await client.sendRequest(ExecuteCommandRequest.type, { command: 'knot.cleanProject', arguments: [targetUri] });
            }
        })
    );

    context.subscriptions.push(
        commands.registerCommand('knot.formatChunk', async () => {
            if (!client || !window.activeTextEditor) return;
            const editor = window.activeTextEditor;
            const uri = editor.document.uri.toString();
            const position = editor.selection.active;
            try {
                await client.sendRequest('knot/formatChunk', { uri, position: { line: position.line, character: position.character } });
            } catch (e) {
                window.showErrorMessage(`Format Chunk failed: ${e}`);
            }
        })
    );
}

export async function deactivate(): Promise<void> {
    if (client) await client.stop();
}

async function openPreview(outputChannel: any): Promise<void> {
    const editor = window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'knot') return;

    if (!client) {
        window.showErrorMessage('LSP not connected — cannot start preview');
        return;
    }

    const knotUri = editor.document.uri;

    compilationStatusBar.text = '$(sync~spin) Starting Knot preview...';
    compilationStatusBar.show();

    try {
        await window.withProgress(
            { location: ProgressLocation.Notification, title: 'Knot Preview', cancellable: false },
            async (progress) => {
                progress.report({ message: 'Compiling...' });

                const result: any = await client!.sendRequest('knot/startPreview', {
                    uri: knotUri.toString(),
                });

                if (result?.status !== 'ok' || !result?.staticServerPort) {
                    throw new Error(result?.message ?? 'unknown error from knot/startPreview');
                }

                await env.openExternal(Uri.parse(`http://127.0.0.1:${result.staticServerPort}/?task=knot-preview`));

                // Keep focus on the .knot editor
                const knotDoc = await workspace.openTextDocument(knotUri);
                await window.showTextDocument(knotDoc, { viewColumn: ViewColumn.One, preserveFocus: false });

                compilationStatusBar.text = '$(check) Preview ready!';
                setTimeout(() => compilationStatusBar.hide(), 2000);
            }
        );
    } catch (e) {
        compilationStatusBar.hide();
        outputChannel.appendLine(`[preview] Failed: ${e}`);
        window.showErrorMessage(`Failed to start preview: ${e}`);
    }
}

async function buildProject(outputChannel: any): Promise<void> {
    const editor = window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'knot') return;

    const knotPath = editor.document.uri.fsPath;
    const projectRoot = findProjectRoot(path.dirname(knotPath));
    if (!projectRoot) {
        window.showErrorMessage('Could not find knot.toml');
        return;
    }

    compilationStatusBar.text = '$(sync~spin) Building PDF...';
    compilationStatusBar.show();

    try {
        await window.withProgress(
            { location: ProgressLocation.Notification, title: 'Knot Build', cancellable: false },
            async (progress) => {
                progress.report({ message: 'Compiling to PDF...' });
                const knotBinary = resolveBinaryPath('knot', outputChannel);
                await runKnotCommand(knotBinary, ['build'], outputChannel, projectRoot);
            }
        );
        compilationStatusBar.text = '$(check) PDF built!';
        setTimeout(() => compilationStatusBar.hide(), 3000);
        window.showInformationMessage('PDF built successfully!');
    } catch (e) {
        compilationStatusBar.hide();
        outputChannel.appendLine(`[build] Failed: ${e}`);
        window.showErrorMessage(`Build failed: ${e}`);
    }
}

async function jumpToKnotSource(outputChannel: any): Promise<void> {
    const editor = window.activeTextEditor;
    if (!editor) return;
    const doc = editor.document;
    if (!isKnotCompiledTyp(doc.fileName)) return;
    const typLine = editor.selection.active.line;
    try {
        const knotBinary = resolveBinaryPath('knot', outputChannel);
        const result = await runKnotCommand(knotBinary, ['jump-to-source', doc.fileName, (typLine + 1).toString()], outputChannel);
        if (result && result.includes(':')) {
            const [file, line] = result.split(':');
            const pos = new Position(parseInt(line, 10) - 1, 0);
            const targetDoc = await workspace.openTextDocument(Uri.file(file));
            await window.showTextDocument(targetDoc, { selection: new Range(pos, pos), viewColumn: ViewColumn.One });
        }
    } catch (e) { /* ignore */ }
}
