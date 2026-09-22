const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { transformSync } = require('esbuild');
const compiled = transformSync(fs.readFileSync('src/utils.ts', 'utf8'), { loader: 'ts', format: 'cjs' }).code;
const moduleObject = { exports: {} };
new Function('module', 'exports', 'require', compiled)(moduleObject, moduleObject.exports, name => name === 'vscode' ? {} : require(name));
const { runKnotCommand } = moduleObject.exports;

test('successful commands keep stdout separate and log warnings', async () => {
    const lines = [];
    const stdout = await runKnotCommand(process.execPath, ['-e', 'console.log("result"); console.error("warning: example")'], { appendLine: line => lines.push(line) });
    assert.equal(stdout, 'result');
    assert.deepEqual(lines, ['warning: example']);
});
test('failed commands preserve multiline diagnostics and reject', async () => {
    const lines = [];
    await assert.rejects(runKnotCommand(process.execPath, ['-e', 'console.error("error: missing\\nmain.typ:12:3\\nhelp: define it"); process.exit(2)'], { appendLine: line => lines.push(line) }));
    assert.match(lines.join('\n'), /error: missing\nmain.typ:12:3\nhelp: define it/);
});
