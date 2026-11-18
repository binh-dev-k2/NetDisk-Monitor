import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(readFileSync(path.join(root, 'package.json'), 'utf8'));
const jsVersions = {
  '@tauri-apps/api': '2.12.5',
  '@tauri-apps/cli': '2.12.5',
  '@tauri-apps/plugin-opener': '2.7.3',
};
const rustVersions = {
  tauri: '2.12.6',
  'tauri-build': '2.7.5',
  'tauri-plugin-opener': '2.7.3',
  'tauri-plugin-single-instance': '2.5.5',
};

test('JavaScript host dependencies are exactly pinned to the approved cohort', () => {
  for (const [name, version] of Object.entries(jsVersions)) {
    assert.equal(manifest.dependencies?.[name] ?? manifest.devDependencies?.[name], version, name);
  }
  assert.equal(manifest.packageManager, 'pnpm@11.21.0');
});

test('installed JavaScript host versions match their declarations', () => {
  for (const [name, version] of Object.entries(jsVersions)) {
    const installed = JSON.parse(readFileSync(path.join(root, 'node_modules', name, 'package.json'), 'utf8'));
    assert.equal(installed.version, version, name);
  }
});

test('Rust host requirements are pinned, synchronized and supported by the declared MSRV', () => {
  const result = spawnSync('cargo', ['metadata', '--manifest-path', 'src-tauri/Cargo.toml',
    '--offline', '--no-deps', '--format-version', '1'], { cwd: root, encoding: 'utf8', windowsHide: true });
  assert.equal(result.status, 0, result.stderr || result.error?.message);
  const metadata = JSON.parse(result.stdout);
  const app = metadata.packages.find(item => item.name === 'netdisk-monitor');
  assert.ok(app, 'app package exists');
  for (const [name, version] of Object.entries(rustVersions)) {
    const dependency = app.dependencies.find(item => item.name === name);
    assert.equal(dependency?.req, `=${version}`, name);
  }
  const [major, minor] = app.rust_version.split('.').map(Number);
  assert.ok(major > 1 || (major === 1 && minor >= 90), 'host cohort requires Rust >=1.90');
  assert.equal(rustVersions.tauri.split('.').slice(0, 2).join('.'), jsVersions['@tauri-apps/api'].split('.').slice(0, 2).join('.'));
  assert.equal(rustVersions['tauri-plugin-opener'], jsVersions['@tauri-apps/plugin-opener']);
});
