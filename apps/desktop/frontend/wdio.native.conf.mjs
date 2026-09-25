import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const frontend = dirname(fileURLToPath(import.meta.url));
const binary = resolve(frontend, '../src-tauri/target/debug', process.platform === 'win32' ? 'rawweave-desktop.exe' : 'rawweave-desktop');
// A large, non-square source so the preview fit and scope layout are exercised
// against a realistic image rather than a 32x32 icon.
const imagePath = resolve(frontend, '../../../test-data/images/common/gracie-allen-portrait.jpg');
const imageName = imagePath.replace(/^.*[\\/]/, '');

const sessionFixture = {
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
  // One queued fixture so the batch suite can build a real job without a folder dialog.
  queue: {
    items: [{
      id: 'fixture-1',
      path: imagePath,
      name: imageName,
      source: {
        path: imagePath,
        name: imageName,
        kind: 'file',
        extension: 'jpg',
        size: 0,
        modifiedTime: null,
        rating: null,
        flag: 'none',
        metadata: null,
        thumbnail: null,
      },
      rating: null,
      flag: 'none',
      order: 0,
      workflowBinding: null,
      overrides: {},
      processingStatus: 'pending',
      outputStatus: 'not-started',
      errors: [],
    }],
    currentPath: imagePath,
    selectedPaths: [],
  },
  testSet: { currentPath: null },
  workflow: { selected: null, unsavedWorkingCopy: null },
  viewer: { targets: { A: { nodeId: 'output', outputPort: 'image' }, B: null } },
  batch: { jobId: null, statePath: null },
  imageSets: [],
  activeImageSetId: null,
  panelLayout: 'default',
};

/**
 * Writes the shared starting state for the native run.
 *
 * A large, non-square source so the preview fit and scope layout are exercised
 * against a realistic image rather than a 32x32 icon, plus one queued fixture so
 * the batch spec can build a real job without opening a folder dialog.
 */
function createAppHome() {
  const home = mkdtempSync(join(tmpdir(), 'rawweave-wdio-'));
  const sessionPath = join(home, 'config', 'com.rawweave.editor', 'browser-session.json');
  mkdirSync(dirname(sessionPath), { recursive: true });
  writeFileSync(sessionPath, JSON.stringify(sessionFixture));
  return home;
}

const appHome = createAppHome();
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
