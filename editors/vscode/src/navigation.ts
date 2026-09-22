import * as fs from 'fs';

// Protocol shared with knot-core::sync::GENERATED_MARKER. Keep it at byte zero.
export const GENERATED_MARKER = '// #KNOT-GENERATED version=1';

export function isKnotCompiledTyp(filePath: string): boolean {
    let fd: number | undefined;
    try {
        fd = fs.openSync(filePath, 'r');
        const buffer = Buffer.alloc(GENERATED_MARKER.length + 2);
        const read = fs.readSync(fd, buffer, 0, buffer.length, 0);
        return buffer.subarray(0, read).toString('utf8').split(/\r?\n/, 1)[0] === GENERATED_MARKER;
    } catch {
        return false;
    } finally {
        if (fd !== undefined) fs.closeSync(fd);
    }
}

/** Editor location with a zero-based line, decoded from the CLI's JSON protocol. */
export interface NavigationLocation { file: string; line: number }

export function parseNavigationLocation(stdout: string): NavigationLocation {
    const value: unknown = JSON.parse(stdout);
    if (!value || typeof value !== 'object' || !('file' in value) || !('line' in value)
        || typeof value.file !== 'string' || !value.file
        || typeof value.line !== 'number' || !Number.isSafeInteger(value.line) || value.line < 1) {
        throw new Error('Invalid navigation location returned by Knot');
    }
    // Rust's canonicalize returns Windows verbatim paths. Uri.file expects a
    // drive path or UNC path, not the Windows device namespace prefix.
    let file = value.file;
    if (file.startsWith('\\\\?\\UNC\\')) file = '\\\\' + file.slice(8);
    else if (/^\\\\\?\\[a-zA-Z]:\\/.test(file)) file = file.slice(4);
    return { file, line: value.line - 1 };
}
