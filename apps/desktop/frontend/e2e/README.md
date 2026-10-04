# Desktop end-to-end tests

Run the fast frontend suite from `apps/desktop/frontend`:

```sh
pnpm run test:e2e:browser
```

WebdriverIO starts Vite and opens the app in headless Chrome. These tests use the in-memory platform adapters and cover the Build / Preview, Browse / Queue, Batch, and Integrations workspaces, along with graph editing, library node drag-and-drop placement and Undo/Redo, parameter and port exposure, drag-created connections, subgraph scopes, workflow import, canvas visibility after workspace changes, saved workflow viewport fitting, canvas refitting after Open Image, node handle alignment, viewer modes, batch fields, and keyboard help. On Linux, set `CHROME_BINARY` if Chrome is installed outside `/usr/bin/google-chrome-stable`.

Build and run the real Tauri suite:

```sh
pnpm run build:e2e:native
pnpm run test:e2e:native
```

The build enables the `wdio-e2e` Rust feature, installs the frontend WebdriverIO plugin, and grants the test window the WebdriverIO permissions. The normal desktop build does not include these plugins or permissions. The native suite uses an isolated app data directory and a checked-in image fixture. It checks the Rust command bridge, restored image source, binary preview protocol, image scopes, batch processing (job creation and pinned workflow hash, preflight, dry run, run to completion, and written output), and the workflow backend: graph edits made through the UI, parameter and port exposure, subgraph scope navigation, blueprint export/instantiate round trips, rejection of invalid edits, and content hashes. WebKitGTK's embedded driver does not deliver pointer actions, so canvas drag-connections are covered in browser mode and the native suite exercises the same Rust connection commands directly; library node-drop coverage dispatches HTML drag events in the real WebView to verify zoom-correct placement, Rust creation and Undo/Redo, rather than claiming physical drag coverage. For the same reason it selects batch recipe options through React's value setter instead of `selectByAttribute`. The runner enables preview diagnostics and captures frontend and backend logs when the test fails.

The native suite currently uses the embedded WebDriver provider on port 4445. It may print a `WebKitWebDriver not found` warning even when the embedded provider connects and the suite passes.
