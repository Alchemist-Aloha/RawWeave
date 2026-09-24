import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const frontend = dirname(fileURLToPath(import.meta.url));
const binary = resolve(frontend, '../src-tauri/target/debug', process.platform === 'win32' ? 'rawweave-desktop.exe' : 'rawweave-desktop');
const appHome = mkdtempSync(join(tmpdir(), 'rawweave-wdio-'));
const imagePath = resolve(frontend, '../../../test-data/images/common/pngsuite-rgb8.png');
const sessionPath = join(appHome, 'config', 'com.rawweave.editor', 'browser-session.json');
mkdirSync(dirname(sessionPath), { recursive: true });
writeFileSync(sessionPath, JSON.stringify({
  version: 1,
  browser: {
    currentFolder: '',
    view: {
      sort: { by: 'name', direction: 'asc' },
      filter: { query: '', rating: 'any', flag: 'any' },
      thumbnailSize: 'medium',
    },
    selectedPaths: [],
  },
  queue: { items: [], currentPath: imagePath, selectedPaths: [] },
  testSet: { currentPath: null },
  workflow: { selected: null, unsavedWorkingCopy: null },
  viewer: { targets: { A: { nodeId: 'output', outputPort: 'image' }, B: null } },
  batch: { jobId: null, statePath: null },
  imageSets: [],
  activeImageSetId: null,
  panelLayout: 'default',
}));
process.env.XDG_DATA_HOME = join(appHome, 'data');
process.env.XDG_CONFIG_HOME = join(appHome, 'config');
process.env.WEBKIT_DISABLE_DMABUF_RENDERER = '1';
process.env.RAWWEAVE_PREVIEW_DIAGNOSTICS = '1';

export const config = {
  runner: 'local',
  specs: ['./e2e/native/**/*.spec.mjs'],
  maxInstances: 1,
  logLevel: 'warn',
  framework: 'mocha',
  reporters: ['spec'],
  waitforTimeout: 15_000,
  mochaOpts: { timeout: 60_000 },
  services: [[
    '@wdio/tauri-service',
    {
      appBinaryPath: binary,
      driverProvider: 'embedded',
      embeddedPort: 4445,
      captureBackendLogs: true,
      captureFrontendLogs: true,
      startTimeout: 60_000,
    },
  ]],
  capabilities: [{
    browserName: 'tauri',
    'tauri:options': { application: binary },
  }],
  onComplete() {
    rmSync(appHome, { recursive: true, force: true });
  },
};
