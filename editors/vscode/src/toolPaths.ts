import * as path from 'path';
import * as fs from 'fs';

/** Discover binaries only within extension directories provided by the editor. */
export function bundledToolPaths(extensionPath: (id: string) => string | undefined,
    platform: NodeJS.Platform = process.platform): Record<string, string> {
    const suffix = platform === 'win32' ? '.exe' : '';
    const result: Record<string, string> = {};
    for (const [key, id, candidates] of [
        ['tinymistPath', 'myriad-dreamin.tinymist', [['out', `tinymist${suffix}`]]],
        ['airPath', 'posit.air-vscode', [['bundled', 'bin', `air${suffix}`], ['bin', `air${suffix}`]]],
        ['ruffPath', 'charliermarsh.ruff', [['bundled', 'libs', 'bin', `ruff${suffix}`]]],
    ] as const) {
        const root = extensionPath(id);
        if (!root) continue;
        const executable = candidates.map(parts => path.join(root, ...parts))
            .find(candidate => fs.existsSync(candidate) && fs.statSync(candidate).isFile());
        if (executable) result[key] = executable;
    }
    return result;
}
