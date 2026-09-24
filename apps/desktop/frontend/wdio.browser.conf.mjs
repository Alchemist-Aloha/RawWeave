import { createServer } from 'vite';

const devServerUrl = 'http://127.0.0.1:5178/';
process.env.VITE_WDIO_BROWSER = '1';

export const config = {
  runner: 'local',
  specs: ['./e2e/browser/**/*.spec.mjs'],
  maxInstances: 1,
  logLevel: 'error',
  framework: 'mocha',
  reporters: ['spec'],
  waitforTimeout: 10_000,
  mochaOpts: { timeout: 30_000 },
  services: [[
    '@wdio/tauri-service',
    {
      mode: 'browser',
      devServerUrl,
      devServer: async () => {
        const server = await createServer({
          server: { host: '127.0.0.1', port: 5178, strictPort: true },
        });
        await server.listen();
        return { url: devServerUrl, close: () => server.close() };
      },
    },
  ]],
  capabilities: [{
    browserName: 'tauri',
    'goog:chromeOptions': {
      ...(process.platform === 'linux' ? { binary: process.env.CHROME_BINARY ?? '/usr/bin/google-chrome-stable' } : {}),
      args: ['--headless=new', '--no-sandbox', '--disable-dev-shm-usage', '--window-size=1440,900'],
    },
  }],
};
