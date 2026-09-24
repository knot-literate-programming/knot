const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { transformSync } = require('esbuild');
const compiled = transformSync(fs.readFileSync('src/toolPaths.ts', 'utf8'), { loader: 'ts', format: 'cjs' }).code;
const moduleObject = { exports: {} };
new Function('module', 'exports', 'require', compiled)(moduleObject, moduleObject.exports, require);
const { bundledToolPaths } = moduleObject.exports;

test('bundled paths come from the editor registry and handle executable suffixes', () => {
    for (const platform of ['darwin', 'linux', 'win32']) {
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'knot tools '));
        try {
            const suffix = platform === 'win32' ? '.exe' : '';
            const expected = {};
            for (const [key, subpath] of [['tinymistPath', `out/tinymist${suffix}`], ['airPath', `bundled/bin/air${suffix}`], ['ruffPath', `bundled/libs/bin/ruff${suffix}`]]) {
                const file = path.join(dir, subpath);
                fs.mkdirSync(path.dirname(file), { recursive: true });
                fs.writeFileSync(file, 'binary');
                expected[key] = file;
            }
            assert.deepEqual(bundledToolPaths(() => dir, platform), expected);
            assert.deepEqual(bundledToolPaths(() => undefined, platform), {});
        } finally { fs.rmSync(dir, { recursive: true, force: true }); }
    }
});
