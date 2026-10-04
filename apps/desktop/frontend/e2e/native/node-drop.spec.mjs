import { $, $$, browser, expect } from '@wdio/globals';

// WebKitGTK's embedded driver cannot deliver physical pointer drags. Exercise
// HTML drag events in the actual WebView and verify the Rust command bridge.
describe('native node library drop', () => {
  it('adds at the drop point after zoom and restores the node in one Undo/Redo', async () => {
    await $('.react-flow__node').waitForExist();
    const beforeIds = await browser.execute(() => [...document.querySelectorAll('.react-flow__node')].map((node) => node.dataset.id));
    await $('.react-flow__controls-zoomin').click();
    await browser.pause(250);
    const point = await browser.execute(() => {
      const item = [...document.querySelectorAll('.node-library__item')].find((node) => node.textContent.includes('core.exposure'));
      const canvas = document.querySelector('.flow-canvas');
      const rect = canvas.getBoundingClientRect();
      const point = { x: rect.left + rect.width * 0.4, y: rect.top + rect.height * 0.3 };
      const dataTransfer = new DataTransfer();
      item.dispatchEvent(new DragEvent('dragstart', { bubbles: true, dataTransfer }));
      canvas.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer }));
      canvas.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer, clientX: point.x, clientY: point.y }));
      return point;
    });
    await browser.waitUntil(async () => (await $$('.react-flow__node')).length === beforeIds.length + 1);
    const newId = await browser.execute((ids) => [...document.querySelectorAll('.react-flow__node')].find((node) => !ids.includes(node.dataset.id)).dataset.id, beforeIds);
    const bounds = async () => browser.execute((id) => {
      const rect = document.querySelector(`.react-flow__node[data-id="${id}"]`).getBoundingClientRect();
      return { x: rect.left, y: rect.top };
    }, newId);
    const placed = await bounds();
    expect(Math.abs(placed.x - point.x)).toBeLessThan(2);
    expect(Math.abs(placed.y - point.y)).toBeLessThan(2);
    const graph = JSON.parse(await browser.tauri.execute((tauri) => tauri.core.invoke('save_workflow')));
    expect(graph.nodes[newId].type_id).toBe('core.exposure');
    await browser.execute(() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true, cancelable: true })));
    await browser.waitUntil(async () => (await $$('.react-flow__node')).length === beforeIds.length);
    await browser.execute(() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true })));
    await browser.waitUntil(async () => (await $$('.react-flow__node')).length === beforeIds.length + 1);
    expect(await bounds()).toEqual(placed);
    await browser.saveScreenshot('./logs/node-drop-native.png');
  });
});
