import { bundledToolPaths } from './toolPaths';
import { isKnotCompiledTyp, parseNavigationLocation, NavigationLocation } from './navigation';
import { CompilationStatus, CompilationEvent } from './compilationStatus';
// Knot VS Code Extension - LSP Client
import * as path from 'path';
import {
    workspace,
    extensions,
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
    TextEditorDecorationType,
    TextEditor,
    ThemeColor,
    SnippetString,
    OutputChannel,
} from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
    ExecuteCommandRequest,
} from 'vscode-languageclient/node';
import { KnotProjectProvider } from './projectExplorer';
import { resolveBinaryPath, findProjectRoot, runKnotCommand } from './utils';

let client: LanguageClient | undefined;
let compilationStatusBar: StatusBarItem;
const compilationStatus = new CompilationStatus();
let statusTimer: ReturnType<typeof setTimeout> | undefined;

function showStatus(text?: string, hideAfter?: number): void {
    if (statusTimer) clearTimeout(statusTimer);
    statusTimer = undefined;
    if (!compilationStatusBar) return;
    if (!text) { compilationStatusBar.hide(); return; }
    compilationStatusBar.text = text;
    compilationStatusBar.show();
    if (hideAfter) statusTimer = setTimeout(() => compilationStatusBar.hide(), hideAfter);
}

function documentVersions(): Map<string, number> {
    return new Map(workspace.textDocuments.map(doc => [doc.uri.toString(), doc.version]));
}

function renderCompilationStatus(): void {
    const doc = window.activeTextEditor?.document;
    if (!doc || doc.languageId !== 'knot') { showStatus(); return; }
    const state = compilationStatus.forDocument(doc.uri.toString(), doc.version);
    void commands.executeCommand('setContext', 'knot.documentHasChanges', state !== 'success');
    const labels = {
        running: '$(sync~spin) Compiling...', modified: '$(edit) Changes pending',
        success: '$(check) Up to date', failed: '$(warning) Compilation failed',
    };
    showStatus(labels[state], state === 'success' || state === 'failed' ? 3000 : undefined);
}
let suppressAutoSync = false;

type StartPreviewResult =
    | { status: 'ok'; staticServerPort: number; taskId: string }
    | { status: 'error'; message: string };

// ---------------------------------------------------------------------------
// Chunk background decorations
// ---------------------------------------------------------------------------

// Chunk fences as the parser accepts them: optional indentation, any case,
// the 'py' alias, and a label or options after the language.
const CHUNK_START_RE = /^\s*```\{\s*(r|python|py)\b[^}]*\}/i;
const CHUNK_END_RE = /^\s*```\s*$/;

// One background per language, as theme colors contributed in package.json
// (knot.rChunkBackground, knot.pythonChunkBackground): they follow the theme
// and can be changed in 'workbench.colorCustomizations'.
let chunkDecorationTypes: Map<string, TextEditorDecorationType> | undefined;

function ensureChunkDecorationTypes(): Map<string, TextEditorDecorationType> {
    if (!chunkDecorationTypes) {
        chunkDecorationTypes = new Map(
            ['r', 'python'].map((language) => [
                language,
                window.createTextEditorDecorationType({
                    backgroundColor: new ThemeColor(`knot.${language}ChunkBackground`),
                    isWholeLine: true,
                }),
            ])
        );
    }
    return chunkDecorationTypes;
}

function applyChunkDecorations(editor: TextEditor): void {
    if (editor.document.languageId !== 'knot') return;
    const doc = editor.document;
    const ranges = new Map<string, Range[]>([['r', []], ['python', []]]);
    let chunk: { start: number; language: string } | undefined;
    for (let i = 0; i < doc.lineCount; i++) {
        const lineText = doc.lineAt(i).text;
        const start = chunk ? null : CHUNK_START_RE.exec(lineText);
        if (start) {
            const language = start[1].toLowerCase() === 'r' ? 'r' : 'python';
            chunk = { start: i, language };
        } else if (chunk && CHUNK_END_RE.test(lineText)) {
            ranges.get(chunk.language)!.push(new Range(chunk.start, 0, i, lineText.length));
            chunk = undefined;
        }
    }
    for (const [language, type] of ensureChunkDecorationTypes()) {
        editor.setDecorations(type, ranges.get(language)!);
    }
}

let syncDebounceTimer: ReturnType<typeof setTimeout> | undefined;
let forwardSyncTimer: ReturnType<typeof setTimeout> | undefined;

let navigationTimer: ReturnType<typeof setTimeout> | undefined;

async function sourceLocation(file: string, line: string, output: OutputChannel): Promise<NavigationLocation> {
    const binary = resolveBinaryPath('knot', output);
    return parseNavigationLocation(await runKnotCommand(binary, ['jump-to-source', file, line, '--json'], output));
}

async function revealLocation(location: NavigationLocation, viewColumn = ViewColumn.One): Promise<void> {
    const document = await workspace.openTextDocument(Uri.file(location.file));
    const position = new Position(location.line, 0);
    if (navigationTimer) clearTimeout(navigationTimer);
    suppressAutoSync = true;
    try {
        await window.showTextDocument(document, { selection: new Range(position, position), viewColumn, preserveFocus: false });
    } finally {
        // Prevent opening the generated document from immediately navigating back.
        navigationTimer = setTimeout(() => { suppressAutoSync = false; }, 500);
    }
}

/**
 * Handles URIs in the form of vscode://knot-dev.knot/jump?file=...&line=...
 */
class KnotUriHandler implements UriHandler {
    constructor(private outputChannel: OutputChannel) {}

    async handleUri(uri: Uri) {
        if (uri.path === '/jump') {
            const query = new URLSearchParams(uri.query);
            const file = query.get('file');
            const line = query.get('line');

            if (file && line) {
                this.outputChannel.appendLine(`[URI Handler] Jump request for ${file}:${line}`);
                try {
                    await revealLocation(await sourceLocation(file, line, this.outputChannel));
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

    // Initialize: no pending changes yet (grays out the Run button).
    await commands.executeCommand('setContext', 'knot.documentHasChanges', false);

    // Apply chunk background decorations and keep them updated.
    window.visibleTextEditors.forEach(applyChunkDecorations);
    context.subscriptions.push(
        window.onDidChangeVisibleTextEditors((editors) => editors.forEach(applyChunkDecorations))
    );
    context.subscriptions.push(
        window.onDidChangeActiveTextEditor((editor) => {
            if (editor) applyChunkDecorations(editor);
            renderCompilationStatus();
        })
    );
    context.subscriptions.push(
        workspace.onDidChangeTextDocument((event) => {
            for (const editor of window.visibleTextEditors) {
                if (editor.document === event.document) {
                    applyChunkDecorations(editor);
                }
            }
        })
    );

    // Activate Run button whenever the user edits a .knot document.
    context.subscriptions.push(
        workspace.onDidChangeTextDocument((event) => {
            if (event.document.languageId === 'knot' && event.contentChanges.length > 0) {
                compilationStatus.edit(event.document.uri.toString());
                renderCompilationStatus();
            }
        })
    );

    // Also activate on first open (document not yet compiled in this session).
    context.subscriptions.push(
        workspace.onDidOpenTextDocument((doc) => {
            if (doc.languageId === 'knot') {
                commands.executeCommand('setContext', 'knot.documentHasChanges', true);
            }
        })
    );

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
                    const location = await sourceLocation(typFileName, (typLine + 1).toString(), outputChannel);
                    const active = window.activeTextEditor;
                    if (active?.document.fileName !== typFileName || active.selection.active.line !== typLine) return;
                    // Keep the generated editor open; navigation must not discard user edits.
                    await revealLocation(location);
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
                } catch {
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

            const knotRelFile = path.relative(projectRoot, knotFilePath);
            try {
                const knotBinary = resolveBinaryPath('knot', outputChannel);
                // Let Rust resolve knot.toml; no second, partial TOML parser here.
                const result = await runKnotCommand(knotBinary, ['jump-to-typ', projectRoot, knotRelFile, (knotLine + 1).toString(), '--json'], outputChannel);
                await revealLocation(parseNavigationLocation(result), ViewColumn.Active);
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
        const toolPaths = bundledToolPaths(id => extensions.getExtension(id)?.extensionPath);
        for (const name of ['air', 'ruff']) {
            const configured = config.get<string>(`formatter.${name}.path`);
            if (configured && configured !== name) toolPaths[`${name}Path`] = configured;
        }
        const clientOptions: LanguageClientOptions = {
            initializationOptions: toolPaths,
            documentSelector: [{ scheme: 'file', language: 'knot' }],
            synchronize: { configurationSection: 'knot', fileEvents: workspace.createFileSystemWatcher('**/*.knot') },
            outputChannel: outputChannel,
        };

        client = new LanguageClient('knotLanguageServer', 'Knot Language Server', serverOptions, clientOptions);

        // Reject obsolete generations and verify versions locally: a text change
        // can reach VS Code before its didChange reaches the server.
        client.onNotification('knot/compilationStarted', (params: CompilationEvent) => {
            compilationStatus.begin(params, true, documentVersions());
            renderCompilationStatus();
        });
        client.onNotification('knot/compilationInvalidated', (params: CompilationEvent) => {
            compilationStatus.begin(params, false, documentVersions());
            renderCompilationStatus();
        });
        client.onNotification('knot/compilationComplete', (params: CompilationEvent & { success: boolean }) => {
            compilationStatus.complete(params, documentVersions());
            renderCompilationStatus();
        });

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

    context.subscriptions.push(
        commands.registerCommand('knot.insertChunkR', () => insertChunk('r'))
    );

    context.subscriptions.push(
        commands.registerCommand('knot.insertChunkPython', () => insertChunk('python'))
    );
}

function insertChunk(lang: string): void {
    const editor = window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'knot') return;
    // Insert a fenced code block and place the cursor on the blank line inside.
    editor.insertSnippet(new SnippetString(`\`\`\`{${lang}}\n$0\n\`\`\`\n`));
}

export async function deactivate(): Promise<void> {
    if (client) await client.stop();
    chunkDecorationTypes?.forEach((type) => type.dispose());
}

async function openPreview(outputChannel: OutputChannel): Promise<void> {
    const editor = window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'knot') return;

    if (!client) {
        window.showErrorMessage('LSP not connected — cannot start preview');
        return;
    }

    const knotUri = editor.document.uri;

    showStatus('$(sync~spin) Starting Knot preview...');

    try {
        await window.withProgress(
            { location: ProgressLocation.Notification, title: 'Knot Preview', cancellable: false },
            async (progress) => {
                progress.report({ message: 'Compiling...' });

                const result = await client!.sendRequest<StartPreviewResult>('knot/startPreview', {
                    uri: knotUri.toString(),
                });

                if (result?.status !== 'ok' || !result?.staticServerPort) {
                    throw new Error(result?.status === 'error' ? result.message : 'unknown error from knot/startPreview');
                }

                await env.openExternal(Uri.parse(`http://127.0.0.1:${result.staticServerPort}/?task=${encodeURIComponent(result.taskId)}`));

                // Keep focus on the .knot editor
                const knotDoc = await workspace.openTextDocument(knotUri);
                await window.showTextDocument(knotDoc, { viewColumn: ViewColumn.One, preserveFocus: false });

                renderCompilationStatus();
            }
        );
    } catch (e) {
        showStatus();
        outputChannel.appendLine(`[preview] Failed: ${e}`);
        window.showErrorMessage(`Failed to start preview: ${e}`);
    }
}

async function buildProject(outputChannel: OutputChannel): Promise<void> {
    const editor = window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'knot') return;

    const knotPath = editor.document.uri.fsPath;
    const projectRoot = findProjectRoot(path.dirname(knotPath));
    if (!projectRoot) {
        window.showErrorMessage('Could not find knot.toml');
        return;
    }

    showStatus('$(sync~spin) Building PDF...');

    try {
        await window.withProgress(
            { location: ProgressLocation.Notification, title: 'Knot Build', cancellable: false },
            async (progress) => {
                progress.report({ message: 'Compiling to PDF...' });
                const knotBinary = resolveBinaryPath('knot', outputChannel);
                await runKnotCommand(knotBinary, ['build'], outputChannel, projectRoot);
            }
        );
        showStatus('$(check) PDF built!', 3000);
        window.showInformationMessage('PDF built successfully!');
    } catch (e) {
        showStatus();
        outputChannel.appendLine(`[build] Failed: ${e}`);
        outputChannel.show(true);
        window.showErrorMessage('Build failed. See the Knot output channel for details.');
    }
}

async function jumpToKnotSource(outputChannel: OutputChannel): Promise<void> {
    const editor = window.activeTextEditor;
    if (!editor) return;
    const doc = editor.document;
    if (!isKnotCompiledTyp(doc.fileName)) return;
    const typLine = editor.selection.active.line;
    try {
        await revealLocation(await sourceLocation(doc.fileName, (typLine + 1).toString(), outputChannel));
    } catch (error) {
        window.showErrorMessage(`Jump to source failed: ${error}`);
    }
}
