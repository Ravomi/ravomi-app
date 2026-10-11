import { mkdirSync, writeFileSync, readFileSync, readdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';

const root = dirname(fileURLToPath(import.meta.url));
const targets = {
  'linux-x64': ['x86_64-unknown-linux-gnu', 'bun-linux-x64'],
  'linux-arm64': ['aarch64-unknown-linux-gnu', 'bun-linux-arm64'],
  'darwin-arm64': ['aarch64-apple-darwin', 'bun-darwin-arm64'],
  'darwin-x64': ['x86_64-apple-darwin', 'bun-darwin-x64'],
  'win32-x64': ['x86_64-pc-windows-msvc', 'bun-windows-x64'],
};
const target = targets[process.env.RAVOMI_AGENT_TARGET ?? `${process.platform}-${process.arch}`];
if (!target) throw new Error('Unsupported agent engine target');
const directory = resolve(root, '../src-tauri/binaries');
mkdirSync(directory, { recursive: true });
const version = spawnSync('bun', ['--version'], { encoding: 'utf8' });
if (version.error) throw version.error;
if (version.stdout.trim() !== '1.3.14') throw new Error('Agent engine requires Bun 1.3.14');
const name = `ravomi-agent-${target[0]}${target[0].includes('windows') ? '.exe' : ''}`;
const result = spawnSync('bun', ['build', '--compile', '--no-compile-autoload-bunfig', '--no-compile-autoload-dotenv', `--target=${target[1]}`, 'src/main.ts', '--outfile', resolve(directory, name)], { cwd: root, stdio: 'inherit' });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
// Keep the exact lockfile next to the license inventory in distributable resources.
writeFileSync(resolve(directory, 'engine-lock.json'), readFileSync(resolve(root, 'package-lock.json')));
const lock = JSON.parse(readFileSync(resolve(root, 'package-lock.json'), 'utf8'));
const notices = ['Ravomi embedded engine: Pi Agent Core / Pi AI 1.0.4; Bun 1.3.14'];
for (const file of readdirSync(resolve(root, 'licenses'))) {
  notices.push(`\n--- ${file} ---\n${readFileSync(resolve(root, 'licenses', file), 'utf8')}`);
}
for (const [path, pkg] of Object.entries(lock.packages)) {
  if (!path || pkg.dev) continue;
  notices.push(`\n--- ${path} ${pkg.version} (${pkg.license ?? 'see upstream'}) ---`);
  for (const file of readdirSync(resolve(root, path))) {
    if (/^(license|licence|copying|notice)(\.|$)/i.test(file)) {
      notices.push(readFileSync(resolve(root, path, file), 'utf8'));
    }
  }
}
writeFileSync(resolve(directory, 'THIRD-PARTY-NOTICES.txt'), notices.join('\n'));
