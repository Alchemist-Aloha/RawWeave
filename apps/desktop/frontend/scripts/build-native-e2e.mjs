import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const frontend = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const tauri = resolve(frontend, '../src-tauri');
const env = {
  ...process.env,
  VITE_WDIO_TEST: '1',
  TAURI_CONFIG: JSON.stringify({
    app: {
      withGlobalTauri: true,
      security: {
        capabilities: [
          'default',
          {
            identifier: 'wdio-e2e',
            description: 'WebdriverIO test access for the RawWeave editor window.',
            windows: ['main'],
            permissions: ['wdio:default', 'wdio-webdriver:default'],
          },
        ],
      },
    },
  }),
};

for (const [command, args, cwd] of [
  ['pnpm', ['run', 'build'], frontend],
  ['cargo', ['build', '--locked', '--features', 'custom-protocol,wdio-e2e'], tauri],
]) {
  const result = spawnSync(command, args, { cwd, env, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
