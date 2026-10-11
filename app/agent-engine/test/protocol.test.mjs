import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const triples = { 'linux-x64': 'x86_64-unknown-linux-gnu', 'linux-arm64': 'aarch64-unknown-linux-gnu', 'darwin-arm64': 'aarch64-apple-darwin', 'darwin-x64': 'x86_64-apple-darwin', 'win32-x64': 'x86_64-pc-windows-msvc' };
const executable = resolve(fileURLToPath(new URL('../../src-tauri/binaries/', import.meta.url)), `ravomi-agent-${triples[`${process.platform}-${process.arch}`]}${process.platform === 'win32' ? '.exe' : ''}`);

test('compiled engine starts with no external runtime on PATH', () => {
  assert.match(execFileSync(executable, ['--version'], { env: { PATH: '' }, encoding: 'utf8' }), /Pi Agent Core 1.0.4/);
  const models = JSON.parse(execFileSync(executable, ['--models'], { encoding: 'utf8' }));
  assert.ok(models.some(m => m.provider === 'deepseek' && m.id === 'deepseek-flash'));
});

test('private protocol returns session identity and explicit model error', async () => {
  const child = spawn(executable, ['--model', 'unsupported/model'], { stdio: ['pipe', 'pipe', 'pipe'] });
  const records = [];
  let buffer = '';
  try {
    const done = new Promise((resolve, reject) => {
      child.on('error', reject);
      child.stdout.on('data', chunk => {
        buffer += chunk;
        let boundary;
        while ((boundary = buffer.indexOf('\n')) >= 0) {
          records.push(JSON.parse(buffer.slice(0, boundary)));
          buffer = buffer.slice(boundary + 1);
          if (records.length === 2) resolve();
        }
      });
    });
    child.stdin.write('{"id":"state","type":"get_state"}\n{"id":"prompt","type":"prompt","message":"test"}\n');
    let timer;
    try {
      await Promise.race([done, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('protocol timeout')), 5000); })]);
    } finally { clearTimeout(timer); }
    assert.equal(records[0].id, 'state');
    assert.ok(records[0].data.sessionId);
    assert.equal(records[1].success, false);
    assert.match(records[1].error, /不支持模型/);
  } finally { child.kill(); }
});
