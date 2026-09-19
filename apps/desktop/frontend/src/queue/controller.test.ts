import { describe, expect, it } from 'vitest';
import { QueueController } from './controller';
import type { BrowserEntry } from '../browser/types';

function entry(name: string): BrowserEntry {
  return {
    path: `/photos/${name}`,
    name,
    kind: 'file',
    extension: 'jpg',
    size: 100,
    modifiedTime: null,
    rating: null,
    flag: 'none',
    metadata: null,
    thumbnail: null,
  };
}

describe('queue controller', () => {
  it('adds selected files once and chooses the first preview item', () => {
    const controller = new QueueController();
    const first = entry('one.jpg');
    const second = entry('two.jpg');

    controller.addSelection([first, second, first], null);

    expect(controller.state.items.map((item) => item.path)).toEqual(['/photos/one.jpg', '/photos/two.jpg']);
    expect(controller.state.selectedPaths).toEqual(['/photos/one.jpg', '/photos/two.jpg']);
    expect(controller.state.currentPath).toBe('/photos/one.jpg');
  });

  it('keeps queue selection and test-set navigation coherent after removal', () => {
    const controller = new QueueController();
    controller.addSelection([entry('one.jpg'), entry('two.jpg')], null);
    controller.setTestSet(['/photos/two.jpg'], true);
    controller.moveTest('next');
    controller.select(['/photos/two.jpg']);
    controller.remove();

    expect(controller.state.items).toHaveLength(1);
    expect(controller.state.currentPath).toBe('/photos/one.jpg');
    expect(controller.state.testSetCurrentPath).toBeNull();
    expect(controller.state.selectedPaths).toEqual([]);
  });

  it('copies per-image overrides without cloning the workflow binding', () => {
    const controller = new QueueController();
    controller.addSelection([entry('one.jpg'), entry('two.jpg')], {
      id: 'workflow',
      version: '1.0.0',
      hash: 'hash',
    });
    controller.setOverride('/photos/one.jpg', 'exposure', 1.5);
    controller.select(['/photos/two.jpg']);
    controller.copyToSelected('/photos/one.jpg');

    expect(controller.state.items[1].overrides).toEqual({ exposure: 1.5 });
    expect(controller.state.items[1].workflowBinding).toEqual(controller.state.items[0].workflowBinding);
    expect(controller.state.items[1].workflowBinding).not.toBe(controller.state.items[0].workflowBinding);
  });
});
