import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions
} from 'vscode-languageclient/node';
import { bootstrap } from './bootstrap';

let client: LanguageClient;

export async function activate(context: vscode.ExtensionContext) {
    const serverPath = await bootstrap(context);

    if (!serverPath) {
        return;
    }

    const serverOptions: ServerOptions = {
        command: serverPath,
        args: ['lsp'],
        options: {
            shell: true
        }
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'java' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.java')
        }
    };

    client = new LanguageClient(
        'naviscopeLSP',
        'Naviscope Language Server',
        serverOptions,
        clientOptions
    );

    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
