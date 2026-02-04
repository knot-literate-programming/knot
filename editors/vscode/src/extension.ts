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
import { workspace, ExtensionContext, window } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

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
}

export async function deactivate(): Promise<void> {
    if (client) {
        await client.stop();
        console.log('Knot LSP client stopped');
    }
}
