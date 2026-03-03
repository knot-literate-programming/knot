import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import { workspace } from 'vscode';
import { execFile } from 'child_process';
import { promisify } from 'util';

const execFileAsync = promisify(execFile);

/**
 * Run a knot command and return its stdout
 */
export async function runKnotCommand(knotPath: string, args: string[], outputChannel?: any, cwd?: string): Promise<string> {
    try {
        const { stdout } = await execFileAsync(knotPath, args, cwd ? { cwd } : {});
        return stdout.trim();
    } catch (e: any) {
        outputChannel?.appendLine(`Knot command failed: ${knotPath} ${args.join(' ')}\nError: ${e.message}`);
        throw e;
    }
}

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

/**
 * Check whether a file path looks like a knot-compiled .typ file
 */
export function isKnotCompiledTyp(filePath: string): boolean {
    try {
        const fd = fs.openSync(filePath, 'r');
        const buffer = Buffer.alloc(4096);
        const bytesRead = fs.readSync(fd, buffer, 0, 4096, 0);
        fs.closeSync(fd);
        const preview = buffer.subarray(0, bytesRead).toString('utf-8');
        return preview.includes('// BEGIN-FILE') && preview.includes('// #KNOT-SYNC');
    } catch {
        return false;
    }
}
