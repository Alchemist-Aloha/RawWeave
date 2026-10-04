import { $, $$, browser, expect } from '@wdio/globals';

async function dragExposure() {
  const search = await $('input[placeholder="Search nodes"]');
  await search.setValue('Exposure');
  const item = await $('.node-library__item');
  const canvas = await $('.flow-canvas');
  await item.dragAndDrop(canvas);
}

async function expectNodeAtDrop(id) {
  const node = await $(`.react-flow__node[data-id="${id}"]`);
  await node.waitForDisplayed();
  const offset = await browser.execute((nodeId) => {
    const canvas = document.querySelector('.flow-canvas').getBoundingClientRect();
    const node = document.querySelector(`.react-flow__node[data-id="${nodeId}"]`).getBoundingClientRect();
    return { x: Math.abs(node.left - (canvas.left + canvas.width / 2)), y: Math.abs(node.top - (canvas.top + canvas.height / 2)) };
  }, id);
  expect(offset.x).toBeLessThan(5);
  expect(offset.y).toBeLessThan(5);
}

describe('node library drag and drop', () => {
  beforeEach(async () => {
    await browser.url('http://127.0.0.1:5178/');
    await $('.react-flow').waitForDisplayed();
  });

  it('drops at the pointer across viewport zoom and restores placement with Undo/Redo', async () => {
    await dragExposure();
    await expectNodeAtDrop('exposure');
    expect(await $$('.react-flow__node')).toHaveLength(1);
    await $('.react-flow__controls-zoomin').click();
    await browser.pause(250);
    await dragExposure();
    await expectNodeAtDrop('exposure-2');
    expect(await $$('.react-flow__node')).toHaveLength(2);
    await $('.flow-canvas').click({ x: 10, y: 10 });
    await browser.keys(['Control', 'z']);
    await browser.waitUntil(async () => (await $$('.react-flow__node')).length === 1);
    await browser.keys(['Control', 'Shift', 'z']);
    await expectNodeAtDrop('exposure-2');
  });

  it('ignores unknown node types and unrelated drops', async () => {
    const accepted = await browser.execute(() => {
      const canvas = document.querySelector('.flow-canvas');
      return ['application/x-rawweave-node', 'text/plain'].map((type) => {
        const dataTransfer = new DataTransfer();
        dataTransfer.setData(type, 'not-a-registered-node');
        const over = new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer });
        canvas.dispatchEvent(over);
        canvas.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer, clientX: 400, clientY: 300 }));
        return over.defaultPrevented;
      });
    });
    expect(accepted).toEqual([true, false]);
    expect(await $$('.react-flow__node')).toHaveLength(0);
  });
});
