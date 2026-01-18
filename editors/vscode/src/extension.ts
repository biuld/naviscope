import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('naviscope');
    const serverPath = config.get<string>('path') || 'naviscope';

    // The server is implemented in rust and run via the 'lsp' subcommand
    const serverOptions: ServerOptions = {
        command: serverPath,
        args: ['lsp'],
        options: {
            shell: true
        }
    };

    // Options to control the language client
    const clientOptions: LanguageClientOptions = {
        // Register the server for java files
        documentSelector: [{ scheme: 'file', language: 'java' }],
        synchronize: {
            // Notify the server about file changes to '.java' files contained in the workspace
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.java')
        }
    };

    // Create the language client and start the client.
    client = new LanguageClient(
        'naviscopeLSP',
        'Naviscope Language Server',
        serverOptions,
        clientOptions
    );

    // Start the client. This will also launch the server
    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
