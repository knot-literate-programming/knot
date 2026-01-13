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
import { workspace, ExtensionContext, window } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export async function activate(context: ExtensionContext) {
    console.log('Activating Knot extension');

    // Check if LSP is enabled in settings
    const config = workspace.getConfiguration('knot');
    const lspEnabled = config.get<boolean>('lsp.enabled', true);

    if (!lspEnabled) {
        console.log('Knot LSP is disabled in settings');
        return;
    }

    // Get LSP server path from settings
    const lspPath = config.get<string>('lsp.path', 'knot-lsp');

    // Server options - launch knot-lsp
    const serverOptions: ServerOptions = {
        command: lspPath,
        args: [],
        transport: TransportKind.stdio,
    };

    // Client options - configure for .knot files
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'knot' }],
        synchronize: {
            // Notify server of configuration changes
            configurationSection: 'knot',
            // Watch for changes in .knot files
            fileEvents: workspace.createFileSystemWatcher('**/*.knot'),
        },
        outputChannelName: 'Knot Language Server',
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
        console.log('Knot LSP client started successfully');

        // Show a message if Air formatter is available
        const airPath = config.get<string>('formatter.air.path', 'air');
        window.showInformationMessage(
            `Knot extension activated. Formatting with Air (${airPath})`
        );
    } catch (error) {
        console.error('Failed to start Knot LSP client:', error);
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
