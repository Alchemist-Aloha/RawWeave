# Desktop end-to-end tests

Run the fast frontend suite from `apps/desktop/frontend`:

```sh
pnpm run test:e2e:browser
```

WebdriverIO starts Vite and opens the app in headless Chrome. These tests use the in-memory platform adapters and cover the Build / Preview, Browse / Queue, Batch, and Integrations workspaces, along with graph editing, viewer modes, batch fields, and keyboard help. On Linux, set `CHROME_BINARY` if Chrome is installed outside `/usr/bin/google-chrome-stable`.

Build and run the real Tauri suite:

```sh
pnpm run build:e2e:native
pnpm run test:e2e:native
```

The build enables the `wdio-e2e` Rust feature, installs the frontend WebdriverIO plugin, and grants the test window the WebdriverIO permissions. The normal desktop build does not include these plugins or permissions. The native suite uses an isolated app data directory and a checked-in image fixture. It checks the Rust command bridge, restored image source, binary preview protocol, and image scopes in WebKitGTK. The runner enables preview diagnostics and captures frontend and backend logs when the test fails.

The native suite currently uses the embedded WebDriver provider on port 4445. It may print a `WebKitWebDriver not found` warning even when the embedded provider connects and the suite passes.
