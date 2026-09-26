import { $, $$, browser, expect } from '@wdio/globals';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const offscreenWorkflow = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'fixtures', 'workflow-offscreen.json');
const imageFixture = resolve(dirname(fileURLToPath(import.meta.url)), '../../../../../test-data/images/common/pngsuite-rgb8.png');

async function addNode(name) {
  const search = await $('input[placeholder="Search nodes"]');
  await search.setValue(name);
  const item = await $('.node-library__item');
  await item.waitForDisplayed();
  await item.click();
  await $(`[aria-label="${name} node"]`).waitForDisplayed();
}

async function selectMode(label) {
  for (const button of await $$('nav[aria-label="Workspace mode"] button')) {
    if (await button.getText() === label) {
      await button.click();
      return;
    }
  }
  throw new Error(`workspace mode ${label} was not found`);
}

async function canvasGeometry() {
  return browser.execute(() => {
    const canvas = document.querySelector('.flow-canvas');
    const flow = document.querySelector('.react-flow');
    const node = document.querySelector('.react-flow__node');
    const rect = (element) => {
      const bounds = element?.getBoundingClientRect();
      return bounds ? { width: bounds.width, height: bounds.height } : null;
    };
    return { canvas: rect(canvas), flow: rect(flow), node: rect(node) };
  });
}

describe('workflow canvas layout', () => {
  beforeEach(async () => {
    await browser.url('http://127.0.0.1:5178/');
    await browser.execute(() => window.localStorage.removeItem('rawweave.dock'));
    await browser.refresh();
    await $('.react-flow').waitForDisplayed();
  });

  afterEach(async () => {
    // Panel visibility is persisted, so reset it between tests.
    await browser.execute(() => window.localStorage.removeItem('rawweave.dock'));
  });

  it('keeps the graph visible after workspace changes and viewport resizes', async () => {
    await addNode('Exposure');
    for (const width of [1440, 900, 640, 1440]) {
      await browser.setWindowSize(width, 900);
      for (const mode of ['Browse / Queue', 'Batch', 'Integrations']) {
        await selectMode(mode);
        await selectMode('Build / Preview');
        const geometry = await canvasGeometry();
        expect(geometry.canvas?.width).toBeGreaterThan(100);
        expect(geometry.canvas?.height).toBeGreaterThan(100);
        expect(geometry.flow?.width).toBeGreaterThan(100);
        expect(geometry.flow?.height).toBeGreaterThan(100);
        expect(geometry.node?.width).toBeGreaterThan(100);
        expect(geometry.node?.height).toBeGreaterThan(20);
      }
    }
  });

  it('positions connection handles on their own port rows', async () => {
    await addNode('Exposure');
    const offsets = await browser.execute(() => [...document.querySelectorAll('.graph-node__port')]
      .map((port) => {
        const handle = port.querySelector('.react-flow__handle');
        if (!handle) return null;
        const row = port.getBoundingClientRect();
        const point = handle.getBoundingClientRect();
        return Math.abs((point.top + point.height / 2) - (row.top + row.height / 2));
      })
      .filter((offset) => offset !== null));
    expect(offsets.length).toBeGreaterThanOrEqual(3);
    for (const offset of offsets) expect(offset).toBeLessThan(3);
  });

  it('fills the preview panel without leaving a blank gap', async () => {
    // Browser mode has no decoded preview, so the scope strip is absent; the
    // image row must then take the whole panel instead of leaving a gap.
    const geometry = await browser.execute(() => {
      const section = document.querySelector('.viewer-section');
      const style = section ? getComputedStyle(section) : null;
      const grid = document.querySelector('.viewer-grid');
      return {
        rows: style?.gridTemplateRows ?? '',
        sectionHeight: Math.round(section?.getBoundingClientRect().height ?? 0),
        toolbar: Math.round(document.querySelector('.viewer-section__toolbar')?.getBoundingClientRect().height ?? 0),
        grid: Math.round(grid?.getBoundingClientRect().height ?? 0),
        scopes: document.querySelectorAll('.viewer-scopes').length,
      };
    });
    // A single compact toolbar row instead of a wrapped block.
    expect(geometry.toolbar).toBeLessThan(60);
    expect(geometry.scopes).toBe(0);
    // The image fills everything the toolbar and padding do not use.
    expect(geometry.grid).toBeGreaterThan(geometry.sectionHeight - geometry.toolbar - 30);
    const rows = geometry.rows.split(' ').map((value) => Number.parseFloat(value));
    expect(rows.reduce((total, value) => total + value, 0)).toBeLessThanOrEqual(geometry.sectionHeight);
    expect(rows[0]).toBeLessThan(60);
  });

  it('hosts the preview in the right dock and collapses every section', async () => {
    const canvasWidth = () => browser.execute(() => Math.round(document.querySelector('.flow-canvas')?.getBoundingClientRect().width ?? 0));
    const rightWidth = () => browser.execute(() => Math.round(document.querySelector('.dock--right')?.getBoundingClientRect().width ?? 0));

    expect(await browser.execute(() => Boolean(document.querySelector('.dock--right .viewer-section')))).toBe(true);

    const initialCanvas = await canvasWidth();
    await $('[aria-label="Collapse Nodes panel"]').click();
    await expect($('[aria-label="Expand Nodes panel"]')).toBeDisplayed();
    await expect($('.panel--library')).not.toExist();
    expect(await canvasWidth()).toBeGreaterThan(initialCanvas);

    await $('[aria-label="Collapse preview panel"]').click();
    await $('[aria-label="Collapse source panel"]').click();
    await expect($('.viewer-section')).toHaveElementClass(expect.stringContaining('viewer-section--collapsed'));

    // The right dock is resizable by dragging its splitter.
    const before = await rightWidth();
    const splitter = await $('[aria-label="Resize right panel"]');
    const box = await splitter.getLocation();
    const start = { x: Math.round(box.x + 2), y: Math.round(box.y + 120) };
    await browser.performActions([{
      type: 'pointer',
      id: 'mouse',
      parameters: { pointerType: 'mouse' },
      actions: [
        { type: 'pointerMove', duration: 0, x: start.x, y: start.y, origin: 'viewport' },
        { type: 'pointerDown', button: 0 },
        { type: 'pointerMove', duration: 120, x: start.x - 80, y: start.y, origin: 'viewport' },
        { type: 'pointerUp', button: 0 },
      ],
    }]);
    expect(await rightWidth()).not.toBe(before);

    await $('[aria-label="Expand Nodes panel"]').click();
    await $('[aria-label="Expand preview panel"]').click();
    await $('[aria-label="Expand source panel"]').click();
    await expect($('.panel--library')).toBeDisplayed();
    await expect($('.viewer-section')).not.toHaveElementClass(expect.stringContaining('viewer-section--collapsed'));
  });

  it('keeps the workspace mounted while dragging a node in a multi-node graph', async () => {
    const inputs = await $$('.topbar__actions input[type="file"]');
    const image = await browser.uploadFile(imageFixture);
    await inputs[1].addValue(image);
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('2 nodes'));

    const item = await $('.node-library__item');
    await item.waitForDisplayed();
    for (let index = 0; index < 8; index += 1) await item.click();
    await browser.waitUntil(async () => (await $$('.react-flow__node')).length >= 10, {
      timeoutMsg: 'the multi-node graph did not build',
    });

    const node = await $('.react-flow__node');
    const box = await node.getLocation();
    const size = await node.getSize();
    const start = { x: Math.round(box.x + size.width / 2), y: Math.round(box.y + 12) };
    const actions = [{ type: 'pointerMove', duration: 0, x: start.x, y: start.y, origin: 'viewport' }, { type: 'pointerDown', button: 0 }];
    for (let step = 1; step <= 10; step += 1) {
      actions.push({ type: 'pointerMove', duration: 40, x: start.x + step * 8, y: start.y + step * 6, origin: 'viewport' });
    }
    actions.push({ type: 'pointerUp', button: 0 });
    await browser.performActions([{ type: 'pointer', id: 'mouse', parameters: { pointerType: 'mouse' }, actions }]);
    await browser.pause(300);

    await expect($('.flow-canvas')).toExist();
    await expect($$('.react-flow__node')).toBeElementsArrayOfSize(10);

    // The drop must be committed: a later editor re-render (the debounced
    // working-copy save) must not snap the node back to where it started.
    const dropped = await node.getLocation();
    expect(Math.abs(dropped.x - box.x) + Math.abs(dropped.y - box.y)).toBeGreaterThan(5);
    await browser.pause(800);
    const settled = await node.getLocation();
    expect(Math.round(settled.x)).toBe(Math.round(dropped.x));
    expect(Math.round(settled.y)).toBe(Math.round(dropped.y));
  });

  it('moves a dragged node on every pointer move, not just on drop', async () => {
    const inputs = await $$('.topbar__actions input[type="file"]');
    await inputs[1].addValue(await browser.uploadFile(imageFixture));
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('2 nodes'));
    const item = await $('.node-library__item');
    await item.waitForDisplayed();
    for (let index = 0; index < 4; index += 1) await item.click();
    await browser.waitUntil(async () => (await $$('.react-flow__node')).length >= 6, {
      timeoutMsg: 'the multi-node graph did not build',
    });

    const node = await $('.react-flow__node');
    const box = await node.getLocation();
    const size = await node.getSize();
    const start = { x: Math.round(box.x + size.width / 2), y: Math.round(box.y + 12) };

    // The whole gesture has to stay in one protocol call: WebDriver resets the
    // pressed state of an input source between calls. Sample the node from inside
    // the page so the movement can be observed while the drag is running.
    await browser.execute(() => {
      const element = document.querySelector('.react-flow__node');
      window.__drag = { samples: [], timer: 0 };
      const sample = () => window.__drag.samples.push({
        transform: getComputedStyle(element).transform,
        dragging: element.classList.contains('dragging'),
      });
      sample();
      window.__drag.timer = window.setInterval(sample, 16);
    });

    const actions = [
      { type: 'pointerMove', duration: 0, x: start.x, y: start.y, origin: 'viewport' },
      { type: 'pointerDown', button: 0 },
    ];
    for (let step = 1; step <= 8; step += 1) {
      actions.push({ type: 'pointerMove', duration: 30, x: start.x + step * 9, y: start.y + step * 6, origin: 'viewport' });
    }
    actions.push({ type: 'pointerUp', button: 0 });
    await browser.performActions([{ type: 'pointer', id: 'mouse', parameters: { pointerType: 'mouse' }, actions }]);
    await browser.pause(300);

    const drag = await browser.execute(() => {
      window.clearInterval(window.__drag.timer);
      return window.__drag.samples;
    });
    const distinct = new Set(drag.map((sample) => sample.transform));
    // Dragging in the middle of the gesture, not only when it ends: a single
    // final transform would mean the node visually waited for the drop.
    expect(distinct.size).toBeGreaterThan(4);
    expect(distinct.size).toBeLessThanOrEqual(drag.length - 1);
    expect(drag.some((sample) => sample.dragging)).toBe(true);

    // The gesture is committed, so the node stays where it was dropped.
    const dropped = await node.getLocation();
    expect(Math.abs(dropped.x - box.x) + Math.abs(dropped.y - box.y)).toBeGreaterThan(5);
    await browser.pause(800);
    const settled = await node.getLocation();
    expect(Math.round(settled.x)).toBe(Math.round(dropped.x));
    expect(Math.round(settled.y)).toBe(Math.round(dropped.y));
    const after = await browser.execute(() => getComputedStyle(document.querySelector('.react-flow__node')).willChange);
    expect(after).not.toBe('transform');
  });

  it('shows nodes after loading a workflow with distant saved positions', async () => {
    await addNode('Image Input');
    const remote = await browser.uploadFile(offscreenWorkflow);
    const inputs = await $$('.topbar__actions input[type="file"]');
    await inputs[2].addValue(remote);
    await $('.react-flow__node').waitForExist();
    await browser.waitUntil(async () => browser.execute(() => {
      const viewport = document.querySelector('.flow-canvas')?.getBoundingClientRect();
      const node = document.querySelector('.react-flow__node')?.getBoundingClientRect();
      return Boolean(viewport && node
        && node.left < viewport.right && node.right > viewport.left
        && node.top < viewport.bottom && node.bottom > viewport.top);
    }), { timeoutMsg: 'the loaded workflow node never entered the visible canvas' });
  });

  it('shows the default graph after opening an image from a distant workflow view', async () => {
    await addNode('Image Input');
    const workflow = await browser.uploadFile(offscreenWorkflow);
    const inputs = await $$('.topbar__actions input[type="file"]');
    await inputs[2].addValue(workflow);
    await $('.react-flow__node').waitForExist();

    const image = await browser.uploadFile(imageFixture);
    await inputs[1].addValue(image);
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('2 nodes'));
    await browser.waitUntil(async () => browser.execute(() => {
      const viewport = document.querySelector('.flow-canvas')?.getBoundingClientRect();
      const node = document.querySelector('.react-flow__node')?.getBoundingClientRect();
      return Boolean(viewport && node
        && node.left < viewport.right && node.right > viewport.left
        && node.top < viewport.bottom && node.bottom > viewport.top);
    }), { timeoutMsg: 'the default graph node never entered the visible canvas' });
  });
});
