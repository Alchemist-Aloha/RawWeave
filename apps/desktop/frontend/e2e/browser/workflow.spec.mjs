import { $, $$, browser, expect } from '@wdio/globals';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const fixture = (name) => resolve(dirname(fileURLToPath(import.meta.url)), '..', 'fixtures', name);
const nodeElement = (name) => $(`[aria-label="${name} node"]`);

async function clickByText(selector, label) {
  for (const element of await $$(selector)) {
    if ((await element.getText()).trim() === label) {
      await element.click();
      return;
    }
  }
  throw new Error(`${selector} with text "${label}" was not found`);
}

/**
 * Sets the node search box so React observes the change.
 *
 * WebdriverIO's `setValue('')` clears the input through the WebDriver element
 * clear command, which React's value tracker swallows. The DOM then looks empty
 * while React still holds the previous query, so the library stays silently
 * filtered and a later `setValue(name)` can race with React restoring the stale
 * query into the input. Writing through the native value setter and dispatching
 * an input event keeps the DOM and React state in sync.
 */
async function setNodeSearch(value) {
  const search = await $('input[placeholder="Search nodes"]');
  await browser.execute((element, next) => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    setter.call(element, next);
    element.dispatchEvent(new Event('input', { bubbles: true }));
  }, search, value);
  await expect(search).toHaveValue(value);
}

async function addNode(name) {
  await setNodeSearch(name);
  const item = await $('.node-library__item');
  await item.waitForExist();
  await item.click();
  await setNodeSearch('');
}

async function selectedNode(name) {
  await nodeElement(name).click();
  await expect($('aside.panel--inspector h2')).toHaveText(name);
}

describe('workflow editing in browser mode', () => {
  beforeEach(async () => {
    await browser.url('http://127.0.0.1:5178/');
    await $('nav[aria-label="Workspace mode"]').waitForDisplayed();
    await $('input[placeholder="Search nodes"]').waitForDisplayed();
  });

  it('edits a parameter and exposes it as a workflow parameter', async () => {
    await addNode('Exposure');
    await selectedNode('Exposure');
    const value = await $('.parameter input[type="number"]');
    await value.setValue('1.5');
    await expect(value).toHaveValue('1.5');

    await $('button[aria-label="Expose Exposure port"]').click();
    await expect($('button[aria-label="Hide Exposure port"]')).toBeDisplayed();
    await expect($('button[aria-label="Hide Exposure port"]')).toHaveAttribute('aria-pressed', 'true');

    await clickByText('.topbar__actions button', 'Undo');
    await expect($('button[aria-label="Expose Exposure port"]')).toBeDisplayed();
    await expect($('.parameter input[type="number"]')).toHaveValue('1.5');
  });

  it('exposes and hides node input and output ports in the inspector', async () => {
    await addNode('Exposure');
    await selectedNode('Exposure');
    const rows = await $$('section[aria-label="Workflow ports"] .port-row');
    expect(rows.length).toBe(3);
    await expect(rows[0]).toHaveText(expect.stringContaining('In Image'));
    await expect(rows[1]).toHaveText(expect.stringContaining('In Exposure'));
    await expect(rows[2]).toHaveText(expect.stringContaining('Out Image'));

    const inputToggle = await rows[0].$('button');
    await inputToggle.click();
    await expect(inputToggle).toHaveText('Hide');
    await inputToggle.click();
    await expect(inputToggle).toHaveText('Expose');

    const outputToggle = await rows[2].$('button');
    await outputToggle.click();
    await expect(outputToggle).toHaveText('Hide');
  });

  it('connects two nodes by dragging between handles', async () => {
    await addNode('Image Input');
    await addNode('Exposure');
    const source = await $('.react-flow__handle.source[data-nodeid="image-input"][data-handleid="image"]');
    const target = await $('.react-flow__handle.target[data-nodeid="exposure"][data-handleid="image"]');
    await source.waitForDisplayed();
    await target.waitForDisplayed();
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: source })
      .down()
      .move({ origin: target, duration: 250 })
      .up()
      .perform();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 links'));
  });

  it('refuses a connection whose data types do not match', async () => {
    // The library hides nodes with no matching inputs, so show every node.
    const addAnyNode = async (name) => {
      const compatibleOnly = await $('[aria-label="Compatible nodes only"]');
      if (await compatibleOnly.isExisting() && await compatibleOnly.isSelected()) await compatibleOnly.click();
      await addNode(name);
    };
    await addAnyNode('Image Input');
    await addAnyNode('Constant Float');
    await addAnyNode('Exposure');
    const source = await $('.react-flow__handle.source[data-nodeid="constant-float"][data-handleid="value"]');
    const target = await $('.react-flow__handle.target[data-nodeid="exposure"][data-handleid="image"]');
    await source.waitForDisplayed();
    await target.waitForDisplayed();
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: source })
      .down()
      .move({ origin: target, duration: 250 })
      .up()
      .perform();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('0 links'));
  });

  it('reveals the disconnect action when an edge is hovered', async () => {
    await addNode('Image Input');
    await addNode('Exposure');
    const source = await $('.react-flow__handle.source[data-nodeid="image-input"][data-handleid="image"]');
    const target = await $('.react-flow__handle.target[data-nodeid="exposure"][data-handleid="image"]');
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: source })
      .down()
      .move({ origin: target, duration: 250 })
      .up()
      .perform();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 links'));

    await expect($('.edge-disconnect')).not.toHaveElementClass(expect.stringContaining('is-visible'));
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: await $('.react-flow__edge-interaction') })
      .perform();
    await expect($('.edge-disconnect')).toHaveElementClass(expect.stringContaining('is-visible'));

    // The action is a real button, so it also works without a context menu.
    await $('.edge-disconnect').click();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('0 links'));
  });

  it('disconnects an edge from its right-click menu', async () => {
    await addNode('Image Input');
    await addNode('Exposure');
    const source = await $('.react-flow__handle.source[data-nodeid="image-input"][data-handleid="image"]');
    const target = await $('.react-flow__handle.target[data-nodeid="exposure"][data-handleid="image"]');
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: source })
      .down()
      .move({ origin: target, duration: 250 })
      .up()
      .perform();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 links'));

    const edge = await $('.react-flow__edge');
    await edge.waitForExist();
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: edge })
      .down({ button: 'right' })
      .up({ button: 'right' })
      .perform();

    const menu = await $('.context-menu');
    await menu.waitForDisplayed();
    await expect(menu).toHaveText(expect.stringContaining('Disconnect'));
    await clickByText('.context-menu__item', 'Disconnect');
    await expect($('.context-menu')).not.toExist();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('0 links'));
  });

  it('disconnects every edge of a node from its right-click menu', async () => {
    await addNode('Image Input');
    await addNode('Exposure');
    const source = await $('.react-flow__handle.source[data-nodeid="image-input"][data-handleid="image"]');
    const target = await $('.react-flow__handle.target[data-nodeid="exposure"][data-handleid="image"]');
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: source })
      .down()
      .move({ origin: target, duration: 250 })
      .up()
      .perform();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 links'));

    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: await nodeElement('Image Input') })
      .down({ button: 'right' })
      .up({ button: 'right' })
      .perform();
    await $('.context-menu').waitForDisplayed();
    await clickByText('.context-menu__item', 'Disconnect 1 output');
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('0 links'));
  });

  it('offers canvas actions on right-click', async () => {
    await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
      .move({ origin: await $('.react-flow__pane') })
      .down({ button: 'right' })
      .up({ button: 'right' })
      .perform();
    const menu = await $('.context-menu');
    await menu.waitForDisplayed();
    await expect(menu).toHaveText(expect.stringContaining('Fit view'));
    await browser.keys('Escape');
    await expect($('.context-menu')).not.toExist();
  });

  it('cancels the subgraph dialog from the keyboard', async () => {
    await addNode('Exposure');
    await selectedNode('Exposure');
    await clickByText('.library-selection button', 'Create subgraph');
    await expect($('form.subgraph-form')).toBeDisplayed();

    await browser.keys('Escape');
    await expect($('form.subgraph-form')).not.toExist();

    // The dialog can be reopened after the keyboard cancel. Re-query the form:
    // the handle captured before the unmount points at a detached element.
    await clickByText('.library-selection button', 'Create subgraph');
    await expect($('form.subgraph-form')).toBeDisplayed();
    await clickByText('form.subgraph-form button', 'Cancel');
    await expect($('form.subgraph-form')).not.toExist();
  });

  it('creates a subgraph from the selection and navigates between scopes', async () => {
    await addNode('Exposure');
    await selectedNode('Exposure');
    await clickByText('.library-selection button', 'Create subgraph');
    const form = await $('form.subgraph-form');
    await form.waitForDisplayed();
    const fields = await $$('form.subgraph-form input');
    await fields[0].setValue('tone-map');
    await fields[2].setValue('Tone Map');
    await clickByText('form.subgraph-form button', 'Create and open');
    await expect(form).not.toExist();

    await expect($('nav[aria-label="Workflow breadcrumbs"]')).toHaveText(expect.stringContaining('Tone Map'));
    expect((await $$('nav[aria-label="Workflow breadcrumbs"] button')).length).toBe(2);
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));

    await clickByText('nav[aria-label="Workflow breadcrumbs"] button', 'Workflow');
    await expect($('[aria-label="Open Tone Map subgraph"]')).toBeDisplayed();
    expect((await $$('nav[aria-label="Workflow breadcrumbs"] button')).length).toBe(1);

    await $('[aria-label="Open Tone Map subgraph"]').click();
    await expect($('nav[aria-label="Workflow breadcrumbs"]')).toHaveText(expect.stringContaining('Tone Map'));
  });

  it('imports a workflow document through the Open Workflow action', async () => {
    const remote = await browser.uploadFile(fixture('workflow-exposure.json'));
    const inputs = await $$('.topbar__actions input[type="file"]');
    expect(inputs.length).toBe(3);
    await inputs[2].addValue(remote);
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));
    await selectedNode('Exposure');
    await expect($('.parameter input[type="number"]')).toHaveValue('2');
  });

  it('reports dependency health and a content hash that follows edits', async () => {
    await expect($('.workflow-health')).toHaveText(expect.stringContaining('Dependencies ready'));
    const before = await $('.workflow-health code').getText();
    expect(before).toMatch(/^#[0-9a-f]{12}$/);
    await addNode('Exposure');
    await browser.waitUntil(async () => (await $('.workflow-health code').getText()) !== before, {
      timeout: 5_000,
      timeoutMsg: 'the workflow hash did not change after adding a node',
    });
  });
});
