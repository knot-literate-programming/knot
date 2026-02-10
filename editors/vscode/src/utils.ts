import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import { workspace } from 'vscode';

/**
 * Resolve a binary path by looking in common locations (bin, .cargo/bin, workspace)
 */
export function resolveBinaryPath(name: string, outputChannel?: any): string {
    const homeBin = path.join(os.homedir(), 'bin', name);
    const cargoBin = path.join(os.homedir(), '.cargo', 'bin', name);
    
    let workspaceBin: string | undefined;
    if (workspace.workspaceFolders && workspace.workspaceFolders.length > 0) {
        workspaceBin = path.join(workspace.workspaceFolders[0].uri.fsPath, 'target', 'release', name);
    }

    if (fs.existsSync(homeBin)) {
        outputChannel?.appendLine(`Found ${name} in ~/bin: ${homeBin}`);
        return homeBin;
    } else if (fs.existsSync(cargoBin)) {
        outputChannel?.appendLine(`Found ${name} in ~/.cargo/bin: ${cargoBin}`);
        return cargoBin;
    } else if (workspaceBin && fs.existsSync(workspaceBin)) {
        outputChannel?.appendLine(`Found ${name} in workspace target/release: ${workspaceBin}`);
        return workspaceBin;
    } else {
        outputChannel?.appendLine(`${name} not found in common locations, relying on system PATH`);
        return name;
    }
}

/**
 * Find the project root by searching for knot.toml in parent directories
 */
export function findProjectRoot(startDir: string): string | null {
    let currentDir = startDir;
    while (currentDir !== path.dirname(currentDir)) {
        if (fs.existsSync(path.join(currentDir, 'knot.toml'))) return currentDir;
        currentDir = path.dirname(currentDir);
    }
    return fs.existsSync(path.join(currentDir, 'knot.toml')) ? currentDir : null;
}

/**
 * Simple TOML parser to extract the 'main' entry point
 */
export function parseMainFromToml(tomlPath: string): string {
    try {
        const content = fs.readFileSync(tomlPath, 'utf-8');
        const match = content.match(/main\s*=\s*"(.*)"/);
        return match ? match[1] : 'main.knot';
    } catch (e) {
        return 'main.knot';
    }
}
