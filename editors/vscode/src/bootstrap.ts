import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import * as https from 'https';
import * as os from 'os';
import { IncomingMessage } from 'http';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

const BINARY_NAME = 'naviscope';
const REPO_OWNER = 'biuld';
const REPO_NAME = 'naviscope';
// Update this version when bundling a new version of the extension
const EXPECTED_VERSION = '0.5.0';

/**
 * Check if naviscope is available in PATH
 */
async function checkPathForNaviscope(): Promise<string | null> {
    try {
        // Use 'which' on Unix-like systems, 'where' on Windows
        const command = process.platform === 'win32' ? 'where' : 'which';
        const { stdout } = await execAsync(`${command} ${BINARY_NAME}`);
        const pathInPath = stdout.trim().split('\n')[0];
        if (pathInPath && fs.existsSync(pathInPath)) {
            return pathInPath;
        }
    } catch (e) {
        // Command not found in PATH
    }
    return null;
}

export async function bootstrap(context: vscode.ExtensionContext): Promise<string | undefined> {
    // First, check if naviscope is available in PATH
    const pathBinary = await checkPathForNaviscope();
    if (pathBinary) {
        // If found in PATH, use it directly without downloading or checking updates
        return pathBinary;
    }

    // Only download and check updates if naviscope is not in PATH
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
    let shouldDownload = false;
    
    if (fs.existsSync(binaryPath)) {
        const isCompatible = await checkVersion(binaryPath);
        if (!isCompatible) {
            const selection = await vscode.window.showWarningMessage(
                `Naviscope binary version mismatch. Expected v${EXPECTED_VERSION}. Update now?`,
                'Update',
                'Skip'
            );
            if (selection === 'Update') {
                shouldDownload = true;
            }
        }
    } else {
        const selection = await vscode.window.showInformationMessage(
            `Naviscope binary is required. Download automatically to ${binDir}?`,
            'Download',
            'Cancel'
        );
        if (selection === 'Download') {
            shouldDownload = true;
        } else {
            return undefined;
        }
    }

    if (shouldDownload) {
        try {
            if (fs.existsSync(binaryPath)) {
                fs.unlinkSync(binaryPath);
            }
            await downloadBinary(binaryPath);
            vscode.window.showInformationMessage(`Naviscope installed successfully!`);
        } catch (error) {
            vscode.window.showErrorMessage(`Failed to download Naviscope: ${error}`);
            if (fs.existsSync(binaryPath)) {
                fs.unlinkSync(binaryPath);
            }
            return undefined;
        }
    }

    if (fs.existsSync(binaryPath)) {
        return binaryPath;
    }
    return undefined;
}

async function checkVersion(binaryPath: string): Promise<boolean> {
    try {
        const { stdout } = await execAsync(`"${binaryPath}" --version`);
        // Expected output: "naviscope 0.2.0"
        return stdout.includes(EXPECTED_VERSION);
    } catch (e) {
        console.warn('Failed to check version:', e);
        return false;
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
            
            // Handle errors on the file stream itself
            file.on('error', (err) => {
                file.close();
                reject(err);
            });

            downloadWithRedirects(url, file, resolve, reject, destPath, progress);
        });
    });
}

function downloadWithRedirects(url: string, file: fs.WriteStream, resolve: () => void, reject: (err: Error) => void, destPath: string, progress: vscode.Progress<{ message?: string; increment?: number }>) {
    https.get(url, (response) => {
        // Handle redirects
        if ([301, 302, 303, 307, 308].includes(response.statusCode || 0)) {
            const location = response.headers.location;
            if (location) {
                downloadWithRedirects(location, file, resolve, reject, destPath, progress);
                return;
            }
        }
        
        handleResponse(response, file, resolve, reject, destPath, progress);
    }).on('error', (err) => {
        // Close file on network error before response
        file.close();
        reject(err);
    });
}

function handleResponse(response: IncomingMessage, file: fs.WriteStream, resolve: () => void, reject: (err: Error) => void, destPath: string, progress: vscode.Progress<{ message?: string; increment?: number }>) {
    if (response.statusCode !== 200) {
        reject(new Error(`HTTP ${response.statusCode}`));
        return;
    }

    const totalBytes = parseInt(response.headers['content-length'] || '0', 10);
    let downloadedBytes = 0;
    let lastPercentage = 0;

    response.on('data', (chunk) => {
        downloadedBytes += chunk.length;
        if (totalBytes > 0) {
            const percentage = Math.floor((downloadedBytes / totalBytes) * 100);
            const increment = percentage - lastPercentage;
            if (increment > 0) {
                lastPercentage = percentage;
                progress.report({ message: `${percentage}%`, increment });
            }
        } else {
             const downloadedMB = (downloadedBytes / 1024 / 1024).toFixed(1);
             progress.report({ message: `${downloadedMB} MB` });
        }
    });

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
    }
    return null;
}
