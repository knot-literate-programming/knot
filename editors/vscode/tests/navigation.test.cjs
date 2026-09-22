const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { transformSync } = require('esbuild');
const compiled = transformSync(fs.readFileSync('src/navigation.ts', 'utf8'), { loader: 'ts', format: 'cjs' }).code;
const moduleObject = { exports: {} };
new Function('module', 'exports', 'require', compiled)(moduleObject, moduleObject.exports, require);
const { GENERATED_MARKER, isKnotCompiledTyp, parseNavigationLocation } = moduleObject.exports;

test('generated header is recognized with a long preamble and no code chunks', () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'knot-navigation-'));
    try {
        const file = path.join(dir, 'report.typ');
        fs.writeFileSync(file, GENERATED_MARKER + '\r\n' + 'library\n'.repeat(2000) + '// BEGIN-FILE main.knot\nHello\n');
        assert.equal(isKnotCompiledTyp(file), true);
        fs.writeFileSync(file, '// An ordinary document\n' + GENERATED_MARKER + '\n');
        assert.equal(isKnotCompiledTyp(file), false);
        assert.equal(isKnotCompiledTyp(path.join(dir, 'missing.typ')), false);
    } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
test('Rust and extension agree on the generated-file header', () => {
    const source = fs.readFileSync('../../crates/knot-core/src/compiler/sync.rs', 'utf8');
    assert.equal(source.match(/pub const GENERATED_MARKER: &str = "([^"]+)";/)[1], GENERATED_MARKER);
});
test('JSON locations preserve Windows drives, Unicode, spaces and colons', () => {
    for (const file of ['C:\\Projet été\\chapter.knot', '\\\\server\\share\\a.knot', '/tmp/étude: résultats/main.knot']) {
        assert.deepEqual(parseNavigationLocation(JSON.stringify({ file, line: 12 })), { file, line: 11 });
    }
});
test('invalid or non-positive locations never reach the editor', () => {
    for (const value of ['file:12', 'null', '{}', '{"file":"a","line":0}', '{"file":"a","line":1.5}', '{"file":"a","line":"3"}']) {
        assert.throws(() => parseNavigationLocation(value));
    }
});
test('Windows canonical paths are converted to editor drive and UNC paths', () => {
    assert.deepEqual(parseNavigationLocation(JSON.stringify({ file: '\\\\?\\C:\\Projet été\\main.knot', line: 1 })), { file: 'C:\\Projet été\\main.knot', line: 0 });
    assert.deepEqual(parseNavigationLocation(JSON.stringify({ file: '\\\\?\\UNC\\server\\share\\main.knot', line: 1 })), { file: '\\\\server\\share\\main.knot', line: 0 });
});
