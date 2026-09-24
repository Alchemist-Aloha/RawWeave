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
    await $('.react-flow').waitForDisplayed();
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
    await browser.pause(800);

    await expect($('.flow-canvas')).toExist();
    await expect($$('.react-flow__node')).toBeElementsArrayOfSize(10);
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
