const { test } = require('node:test');
const assert = require('node:assert/strict');
const { transformSync } = require('esbuild');
const fs = require('node:fs');
// Exercise the same TypeScript module bundled in the extension, without VS Code.
const compiled = transformSync(fs.readFileSync('src/compilationStatus.ts', 'utf8'), { loader: 'ts', format: 'cjs' }).code;
const exportsObject = { exports: {} };
new Function('module', 'exports', compiled)(exportsObject, exportsObject.exports);
const { CompilationStatus } = exportsObject.exports;
const event = (generation, project = 'a', versions = { main: 1, include: 1 }) => ({ uri: 'main', project, generation, versions });
const versions = new Map([['main', 1], ['include', 1], ['other', 1]]);

test('old completion cannot replace a newer running generation', () => {
    const status = new CompilationStatus();
    status.begin(event(1), true, versions);
    status.begin(event(2), true, versions);
    status.complete({ ...event(1), success: true }, versions);
    assert.equal(status.forDocument('main', 1), 'running');
    status.complete({ ...event(2), success: true }, versions);
    assert.equal(status.forDocument('main', 1), 'success');
});
test('an include edit immediately invalidates the main document', () => {
    const status = new CompilationStatus();
    status.begin(event(1), true, versions);
    status.edit('include');
    status.complete({ ...event(1), success: true }, versions);
    assert.equal(status.forDocument('main', 1), 'modified');
});
test('local versions reject results even before the server receives didChange', () => {
    const status = new CompilationStatus();
    status.begin(event(1), true, versions);
    status.complete({ ...event(1), success: true }, new Map([...versions, ['include', 2]]));
    assert.equal(status.forDocument('main', 1), 'modified');
});
test('invalidations and late start notifications cannot resurrect an old run', () => {
    const status = new CompilationStatus();
    status.begin(event(2), false, versions);
    status.begin(event(1), true, versions);
    status.complete({ ...event(1), success: true }, versions);
    assert.equal(status.forDocument('main', 1), 'modified');
});
test('project generations are independent and failures leave Run enabled', () => {
    const status = new CompilationStatus();
    status.begin(event(5), true, versions);
    const other = event(1, 'b', { other: 1 });
    status.begin(other, true, versions);
    status.complete({ ...other, success: false }, versions);
    assert.equal(status.forDocument('other', 1), 'failed');
    assert.equal(status.forDocument('main', 1), 'running');
});
test('a closed buffer or late start cannot be marked current', () => {
    const status = new CompilationStatus();
    status.begin(event(1), true, new Map([['main', 2]]));
    status.complete({ ...event(1), success: true }, versions);
    assert.equal(status.forDocument('main', 2), 'modified');
});
