import { $, $$, browser, expect } from '@wdio/globals';

async function invoke(command, args) {
  return browser.tauri.execute((tauri, command, args) => tauri.core.invoke(command, args), command, args);
}

async function invokeError(command, args) {
  try {
    await invoke(command, args);
  } catch (error) {
    return String(error?.message ?? error);
  }
  throw new Error(`${command} unexpectedly succeeded`);
}

async function graph() {
  return JSON.parse(await invoke('save_workflow'));
}

async function blueprint() {
  return JSON.parse(await invoke('save_blueprint'));
}

const nodeIds = (value) => Object.keys(value.nodes);

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
 * Sets the node search box so React observes the change. See the browser spec for
 * why `setValue('')` is not enough: WebDriver's element clear command is hidden
 * from React's value tracker, which leaves the library silently filtered.
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
  await $(`[aria-label="${name} node"]`).click();
  await expect($('aside.panel--inspector h2')).toHaveText(name);
}

describe('workflow backend in the real Tauri app', () => {
  it('exposes the workflow command bridge, descriptors, hash, and dependency report', async () => {
    const descriptors = await invoke('node_descriptors');
    const typeIds = descriptors.map((descriptor) => descriptor.type_id);
    expect(typeIds).toEqual(expect.arrayContaining(['core.image-input', 'core.output', 'core.exposure', 'core.invert']));

    const hash = await invoke('workflow_hash');
    expect(hash).toMatch(/^[0-9a-f]{64}$/);

    const report = await invoke('dependency_status');
    expect(report).toEqual(expect.objectContaining({
      available: expect.any(Array),
      missing: expect.any(Array),
      mismatched: expect.any(Array),
      disabledNodes: expect.any(Array),
    }));
  });

  it('restores the ordinary workflow and writes UI node edits to the backend graph', async () => {
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('2 nodes'));
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 links'));
    await expect($('[aria-label="Image Input node"]')).toBeDisplayed();
    await expect($('[aria-label="Output node"]')).toBeDisplayed();

    await addNode('Exposure');
    await addNode('Invert');
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('4 nodes'));
    expect(nodeIds(await graph())).toEqual(expect.arrayContaining(['input', 'output', 'exposure', 'invert']));

    // The canvas drag path is covered in browser mode; WebKitGTK's embedded
    // driver does not deliver pointer actions, so exercise the same Rust
    // command the controller calls when a connection is dropped.
    await invoke('connect_nodes', { fromNode: 'exposure', fromPort: 'image', toNode: 'invert', toPort: 'image' });
    expect((await graph()).edges).toEqual(expect.arrayContaining([
      expect.objectContaining({ from_node: 'exposure', from_port: 'image', to_node: 'invert', to_port: 'image' }),
    ]));

    await invoke('disconnect_nodes', { fromNode: 'exposure', fromPort: 'image', toNode: 'invert', toPort: 'image' });
    expect((await graph()).edges.some((edge) => edge.from_node === 'exposure' && edge.to_node === 'invert')).toBe(false);
  });

  it('persists parameter edits and exposed parameters made through the inspector', async () => {
    await selectedNode('Exposure');
    const value = await $('.parameter input[type="number"]');
    await value.setValue('1.5');
    await value.click();
    await expect(value).toHaveValue('1.5');

    await $('button[aria-label="Expose Exposure port"]').click();
    await expect($('button[aria-label="Hide Exposure port"]')).toBeDisplayed();

    const persisted = await graph();
    expect(persisted.nodes.exposure.parameters.exposure).toEqual({ Float: 1.5 });
    expect(persisted.nodes.exposure.exposed_parameters).toContain('exposure');
  });

  it('removes a node from the canvas and the backend graph', async () => {
    await addNode('Resize');
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('5 nodes'));
    await selectedNode('Resize');
    await $('button[aria-label="Delete Resize"]').click();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('4 nodes'));
    expect(nodeIds(await graph())).not.toContain('resize');
  });

  it('creates a subgraph from the selection and navigates back to the parent scope', async () => {
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

    const child = await blueprint();
    expect(child.identity.id).toBe('tone-map');
    expect(Object.keys(child.graph.nodes)).toEqual(['exposure']);

    await clickByText('nav[aria-label="Workflow breadcrumbs"] button', 'Workflow');
    await expect($('[aria-label="Open Tone Map subgraph"]')).toBeDisplayed();
    const root = await blueprint();
    expect(Object.keys(root.nested_subgraphs)).toContain('tone-map');
    expect(root.subgraph_dependencies.map((dependency) => dependency.id)).toContain('tone-map');
  });

  it('round-trips a blueprint through export and instantiate', async () => {
    const exported = await invoke('export_blueprint');
    expect(JSON.parse(exported).identity.id).toBe('workflow');

    await invoke('add_node', { nodeId: 'scratch', typeId: 'core.invert' });
    expect(nodeIds(await graph())).toContain('scratch');

    await invoke('instantiate_blueprint', { serialized: exported });
    expect(nodeIds(await graph())).not.toContain('scratch');
    expect(nodeIds(await graph())).toEqual(expect.arrayContaining(['input', 'output', 'exposure', 'invert']));
  });

  it('exposes and hides workflow ports through the backend blueprint', async () => {
    await selectedNode('Invert');
    const rows = await $$('section[aria-label="Workflow ports"] .port-row');
    await expect(rows[0]).toHaveText(expect.stringContaining('In Image'));
    await (await rows[0].$('button')).click();
    await expect(await rows[0].$('button')).toHaveText('Hide');

    const exposed = await blueprint();
    expect(exposed.inputs).toEqual(expect.arrayContaining([
      expect.objectContaining({ id: 'input:invert:image', node_id: 'invert', port_id: 'image', direction: 'Input' }),
    ]));

    await (await rows[0].$('button')).click();
    await expect(await rows[0].$('button')).toHaveText('Expose');
    const hidden = await blueprint();
    expect(hidden.inputs).toEqual([]);
  });

  it('rejects invalid edits and retains the previous graph', async () => {
    expect(await invokeError('add_node', { nodeId: 'exposure', typeId: 'core.exposure' }))
      .toMatch(/node 'exposure' already exists/);
    expect(await invokeError('add_node', { nodeId: 'ghost', typeId: 'core.not-a-node' }))
      .toMatch(/node type 'core.not-a-node' is not registered/);
    expect(await invokeError('connect_nodes', {
      fromNode: 'input', fromPort: 'image', toNode: 'output', toPort: 'image',
    })).toMatch(/already has a connection/);

    const before = await invoke('save_workflow');
    expect(await invokeError('load_workflow', { workflow: '{ not json' }))
      .toMatch(/serialization|invalid|expected|EOF|json/i);
    expect(await invoke('save_workflow')).toBe(before);
  });
});

/**
 * Fields are painted by the app, not by the system theme.
 *
 * WebKit paints its own field over any background it is given, so a select in the
 * dark room rendered as a white system box with a light label on it while
 * `getComputedStyle` reported the app's own colours. Dropping the system
 * appearance is what makes the palette reach the screen; if that is ever removed,
 * the batch recipe silently turns white again on the shipped engine.
 *
 * This lives in this file rather than its own because the native run gives every
 * spec file its own app instance over one shared app home, so a third file makes
 * the suite interfere with itself.
 */
describe('fields keep the palette on the shipped engine', () => {
  it('paints the batch recipe with the app colours, not the system theme', async () => {
    for (const button of await $$('nav[aria-label="Workspace mode"] button')) {
      if ((await button.getText()) === 'Batch') {
        await button.click();
        break;
      }
    }
    const select = await $('select[aria-label="Batch output format"]');
    await select.waitForExist();

    const painted = await browser.execute(() => {
      const field = document.querySelector('select[aria-label="Batch output format"]');
      const number = document.querySelector('.batch-panel__fields input');
      const box = document.querySelector('input[type="checkbox"]');
      return {
        appearance: getComputedStyle(field).appearance,
        background: getComputedStyle(field).backgroundColor,
        numberAppearance: number ? getComputedStyle(number).appearance : '',
        checkboxAppearance: box ? getComputedStyle(box).appearance : '',
      };
    });

    expect(painted.appearance).toBe('none');
    expect(painted.numberAppearance).toBe('none');
    // a checkbox is a native control and must stay one
    expect(painted.checkboxAppearance).not.toBe('none');
    expect(painted.background).not.toMatch(/255, 255, 255/);
  });
});
