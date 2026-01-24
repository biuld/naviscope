import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import * as https from 'https';
import * as os from 'os';
import { IncomingMessage } from 'http';

const BINARY_NAME = 'naviscope';
const REPO_OWNER = 'biuld';
const REPO_NAME = 'naviscope';

export async function bootstrap(context: vscode.ExtensionContext): Promise<string | undefined> {
    const naviscopeHome = path.join(os.homedir(), '.naviscope');
    const binDir = path.join(naviscopeHome, 'bin');
    
    if (!fs.existsSync(binDir)) {
        try {
            fs.mkdirSync(binDir, { recursive: true });
        } catch (err) {
            vscode.window.showErrorMessage(`Failed to create directory ${binDir}: ${err}`);
            return undefined;
        }
    }

    const binaryPath = path.join(binDir, BINARY_NAME);
    
    if (fs.existsSync(binaryPath)) {
        return binaryPath;
    }

    const selection = await vscode.window.showInformationMessage(
        `Naviscope binary is required. Download automatically to ${binDir}?`,
        'Download',
        'Cancel'
    );

    if (selection !== 'Download') {
        return undefined;
    }

    try {
        await downloadBinary(binaryPath);
        vscode.window.showInformationMessage(`Naviscope installed successfully!`);
        return binaryPath;
    } catch (error) {
        vscode.window.showErrorMessage(`Failed to download Naviscope: ${error}`);
        if (fs.existsSync(binaryPath)) {
            fs.unlinkSync(binaryPath);
        }
        return undefined;
    }
}

async function downloadBinary(destPath: string): Promise<void> {
    const platform = getPlatformIdentifier();
    if (!platform) {
        throw new Error(`Unsupported platform: ${os.platform()} ${os.arch()}`);
    }

    const url = `https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/latest/download/naviscope-${platform}`;

    return vscode.window.withProgress({
        location: vscode.ProgressLocation.Notification,
        title: "Downloading Naviscope...",
        cancellable: false
    }, (progress) => {
        return new Promise<void>((resolve, reject) => {
            const file = fs.createWriteStream(destPath);
            
            https.get(url, (response) => {
                if (response.statusCode === 302 || response.statusCode === 301) {
                    https.get(response.headers.location!, (redirectResponse) => {
                        handleResponse(redirectResponse, file, resolve, reject, destPath);
                    }).on('error', reject);
                } else {
                    handleResponse(response, file, resolve, reject, destPath);
                }
            }).on('error', reject);
        });
    });
}

function handleResponse(response: IncomingMessage, file: fs.WriteStream, resolve: () => void, reject: (err: Error) => void, destPath: string) {
    if (response.statusCode !== 200) {
        reject(new Error(`HTTP ${response.statusCode}`));
        return;
    }

    response.pipe(file);

    file.on('finish', () => {
        file.close();
        if (process.platform !== 'win32') {
            fs.chmodSync(destPath, '755');
        }
        resolve();
    });

    file.on('error', (err) => {
        reject(err);
    });
}

function getPlatformIdentifier(): string | null {
    const platform = os.platform();
    const arch = os.arch();

    if (platform === 'linux') {
        return 'linux-x86_64';
    }
    if (platform === 'darwin') {
        if (arch === 'arm64') {
            return 'macos-aarch64';
        }
        // Intel Mac is not supported
    }
    return null;
}
