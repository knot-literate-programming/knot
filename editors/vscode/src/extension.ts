// Knot VS Code Extension - LSP Client
//
// This extension provides IDE support for .knot files by connecting to the knot-lsp server.
// Features:
// - R code formatting with Air
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
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let watchProcesses: Map<string, ChildProcess> = new Map();

export async function activate(context: ExtensionContext) {
    const outputChannel = window.createOutputChannel('Knot Extension');
    outputChannel.appendLine('Activating Knot extension...');

    // Check if LSP is enabled in settings
    const config = workspace.getConfiguration('knot');
    const lspEnabled = config.get<boolean>('lsp.enabled', true);

    if (!lspEnabled) {
        outputChannel.appendLine('Knot LSP is disabled in settings');
        return;
    }

    // Get LSP server path from settings
    let lspPath = config.get<string>('lsp.path', 'knot-lsp');
    outputChannel.appendLine(`Configured LSP path: ${lspPath}`);

    // Robust path resolution for knot-lsp
    if (lspPath === 'knot-lsp') {
        const homeBin = path.join(os.homedir(), 'bin', 'knot-lsp');
        const cargoBin = path.join(os.homedir(), '.cargo', 'bin', 'knot-lsp');
        
        // Also look in the workspace target directory for developers
        let workspaceBin: string | undefined;
        if (workspace.workspaceFolders && workspace.workspaceFolders.length > 0) {
            workspaceBin = path.join(workspace.workspaceFolders[0].uri.fsPath, 'target', 'release', 'knot-lsp');
        }

        outputChannel.appendLine(`Checking for knot-lsp in: \n - ${homeBin}\n - ${cargoBin}${workspaceBin ? `\n - ${workspaceBin}` : ''}`);

        if (fs.existsSync(homeBin)) {
            lspPath = homeBin;
            outputChannel.appendLine(`Found knot-lsp in ~/bin: ${lspPath}`);
        } else if (fs.existsSync(cargoBin)) {
            lspPath = cargoBin;
            outputChannel.appendLine(`Found knot-lsp in ~/.cargo/bin: ${lspPath}`);
        } else if (workspaceBin && fs.existsSync(workspaceBin)) {
            lspPath = workspaceBin;
            outputChannel.appendLine(`Found knot-lsp in workspace target/release: ${lspPath}`);
        } else {
            outputChannel.appendLine('knot-lsp not found in common locations, relying on system PATH');
        }
    }

    // Server options - launch knot-lsp
    const serverOptions: ServerOptions = {
        command: lspPath,
        args: [],
        transport: TransportKind.stdio,
    };

    outputChannel.appendLine(`Starting LSP client with command: ${lspPath}`);

    // Get Air path for the server
    let airPath = config.get<string>('formatter.air.path', 'air');
    if (airPath === 'air') {
        const homeAir = path.join(os.homedir(), 'bin', 'air');
        if (fs.existsSync(homeAir)) {
            airPath = homeAir;
        } else {
            // Look in extensions
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

    // Try to find tinymist path to help the server
    let tinymistPath: string | undefined;
    const homeTinymist = path.join(os.homedir(), 'bin', 'tinymist');
    if (fs.existsSync(homeTinymist)) {
        tinymistPath = homeTinymist;
    } else {
        // Look in VS Code extensions
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

    outputChannel.appendLine(`Resolved air path: ${airPath}`);
    outputChannel.appendLine(`Resolved tinymist path: ${tinymistPath || 'not found'}`);

    // Client options - configure for .knot files
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'knot' }],
        synchronize: {
            // Notify server of configuration changes
            configurationSection: 'knot',
            // Watch for changes in .knot files
            fileEvents: workspace.createFileSystemWatcher('**/*.knot'),
        },
        initializationOptions: {
            airPath: airPath,
            tinymistPath: tinymistPath
        },
        outputChannel: outputChannel, // Reuse our debug channel
    };

    // Create and start the client
    client = new LanguageClient(
        'knotLanguageServer',
        'Knot Language Server',
        serverOptions,
        clientOptions
    );

    try {
        await client.start();
        outputChannel.appendLine('Knot LSP client started successfully');

        // Show a message if Air formatter is available
        const airPath = config.get<string>('formatter.air.path', 'air');
        window.showInformationMessage(
            `Knot extension activated. Formatting with Air (${airPath})`
        );
    } catch (error) {
        outputChannel.appendLine(`Failed to start Knot LSP client: ${error}`);
        window.showErrorMessage(
            `Failed to start Knot Language Server. Make sure 'knot-lsp' is installed and in PATH.\n\nError: ${error}`
        );
    }

    // Register preview command
    context.subscriptions.push(
        commands.registerCommand('knot.openPreview', async () => {
            await openPreview(outputChannel);
        })
    );

    // Register clean project command
    context.subscriptions.push(
        commands.registerCommand('knot.cleanProject', async (resource?: Uri) => {
            if (!client) {
                window.showErrorMessage('Knot Language Server is not running');
                return;
            }

            // Determine which file we are cleaning for
            let targetUri: string | undefined;
            
            if (resource) {
                // Called from context menu or title bar with context
                targetUri = resource.toString();
            } else {
                // Called from command palette, use active editor
                if (window.activeTextEditor && window.activeTextEditor.document.languageId === 'knot') {
                    targetUri = window.activeTextEditor.document.uri.toString();
                }
            }

            if (!targetUri) {
                window.showErrorMessage('No active Knot file to clean.');
                return;
            }

            outputChannel.appendLine(`Executing Clean Project command for ${targetUri}...`);
            try {
                await client.sendRequest('workspace/executeCommand', {
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
    if (client) {
        await client.stop();
        console.log('Knot LSP client stopped');
    }

    // Stop all watch processes
    for (const [knotPath, process] of watchProcesses) {
        console.log(`Stopping knot watch for ${knotPath}`);
        process.kill();
    }
    watchProcesses.clear();
}

async function openPreview(outputChannel: any): Promise<void> {
    const editor = window.activeTextEditor;
    if (!editor) {
        window.showErrorMessage('No active editor');
        return;
    }

    const document = editor.document;
    if (document.languageId !== 'knot') {
        window.showErrorMessage('Active file is not a .knot file');
        return;
    }

    const knotPath = document.uri.fsPath;
    const knotDir = path.dirname(knotPath);

    outputChannel.appendLine(`Opening preview for ${knotPath}`);

    // Find project root (directory containing knot.toml)
    const projectRoot = findProjectRoot(knotDir);
    if (!projectRoot) {
        window.showErrorMessage('Could not find knot.toml in parent directories. Make sure you are in a Knot project.');
        return;
    }

    outputChannel.appendLine(`Project root: ${projectRoot}`);

    // PDF is named after the project directory, not the .knot file
    const projectName = path.basename(projectRoot);
    const pdfPath = path.join(projectRoot, `${projectName}.pdf`);

    outputChannel.appendLine(`Expected PDF path: ${pdfPath}`);

    // Check if knot watch is already running for this project
    if (!watchProcesses.has(projectRoot)) {
        outputChannel.appendLine('Starting knot watch...');

        try {
            const watchProcess = spawn('knot', ['watch'], {
                cwd: projectRoot,
                stdio: ['ignore', 'pipe', 'pipe']
            });

            watchProcess.stdout?.on('data', (data) => {
                outputChannel.appendLine(`[knot watch] ${data.toString().trim()}`);
            });

            watchProcess.stderr?.on('data', (data) => {
                outputChannel.appendLine(`[knot watch error] ${data.toString().trim()}`);
            });

            watchProcess.on('error', (error) => {
                outputChannel.appendLine(`Failed to start knot watch: ${error.message}`);
                window.showErrorMessage(`Failed to start knot watch: ${error.message}`);
                watchProcesses.delete(projectRoot);
            });

            watchProcess.on('exit', (code) => {
                outputChannel.appendLine(`knot watch exited with code ${code}`);
                watchProcesses.delete(projectRoot);
            });

            watchProcesses.set(projectRoot, watchProcess);

            // Wait a bit for initial compilation
            await new Promise(resolve => setTimeout(resolve, 1000));
        } catch (error) {
            outputChannel.appendLine(`Error starting knot watch: ${error}`);
            window.showErrorMessage(`Failed to start knot watch: ${error}`);
            return;
        }
    } else {
        outputChannel.appendLine('knot watch already running for this project');
    }

    // Wait for PDF to be generated (with timeout)
    const maxWaitTime = 10000; // 10 seconds
    const startTime = Date.now();

    while (!fs.existsSync(pdfPath)) {
        if (Date.now() - startTime > maxWaitTime) {
            window.showErrorMessage('Timeout waiting for PDF generation. Check the output channel for errors.');
            return;
        }
        await new Promise(resolve => setTimeout(resolve, 200));
    }

    outputChannel.appendLine(`PDF generated at ${pdfPath}`);

    // Open PDF according to user preference
    const config = workspace.getConfiguration('knot');
    const pdfViewer = config.get<string>('preview.pdfViewer', 'vscode');

    if (pdfViewer === 'external') {
        // Open with system default PDF viewer
        outputChannel.appendLine('Opening PDF with external viewer');
        const pdfUri = Uri.file(pdfPath);
        await commands.executeCommand('vscode.open', pdfUri, { viewColumn: -2 }); // Opens externally
    } else {
        // Open in VS Code (requires PDF viewer extension)
        outputChannel.appendLine('Opening PDF in VS Code');
        const pdfUri = Uri.file(pdfPath);
        await commands.executeCommand('vscode.open', pdfUri, { viewColumn: 2 }); // Side by side
    }

    window.showInformationMessage(`Preview opened for ${projectName}`);
}

function findProjectRoot(startDir: string): string | null {
    let currentDir = startDir;

    // Keep going up until we find knot.toml or reach the root
    while (currentDir !== path.dirname(currentDir)) {
        const configPath = path.join(currentDir, 'knot.toml');
        if (fs.existsSync(configPath)) {
            return currentDir;
        }
        currentDir = path.dirname(currentDir);
    }

    // Check root directory as well
    const configPath = path.join(currentDir, 'knot.toml');
    if (fs.existsSync(configPath)) {
        return currentDir;
    }

    return null;
}
