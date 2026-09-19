import { describe, expect, it } from 'vitest';
import { defaultSession, parseSession, serializeSession } from './session';

describe('browser session persistence', () => {
  it('round trips the browse, queue, test-set, workflow, and viewer state', () => {
    const session = defaultSession('/photos');
    session.browser.view.sort = { by: 'modified', direction: 'desc' };
    session.workflow.selected = { id: 'workflow', version: '1.0.0', hash: 'hash' };
    session.workflow.unsavedWorkingCopy = '{"version":1}';
    session.viewer.targets = {
      A: { nodeId: 'display', outputPort: 'display' },
      B: { nodeId: 'output', outputPort: 'image' },
    };
    session.panelLayout = 'split';
    session.queue.currentPath = '/photos/selected.jpg';
    session.queue.items = [{
      id: '/photos/selected.jpg',
      path: '/photos/selected.jpg',
      name: 'selected.jpg',
      source: {
        path: '/photos/selected.jpg',
        name: 'selected.jpg',
        kind: 'file',
        extension: 'jpg',
        size: 1,
        modifiedTime: null,
        rating: 5,
        flag: 'pick',
        metadata: null,
        thumbnail: null,
      },
      rating: 5,
      flag: 'pick',
      order: 0,
      workflowBinding: { id: 'workflow', version: '1.0.0', hash: 'hash' },
      overrides: { 'exposure:exposure': 1.25 },
      processingStatus: 'pending',
      outputStatus: 'not-started',
      errors: [],
      warnings: [],
      testSet: true,
    }];

    expect(parseSession(serializeSession(session))).toEqual(session);
  });

  it('rejects malformed or incompatible sessions instead of reviving partial state', () => {
    expect(parseSession('{"version":999}')).toBeNull();
    expect(parseSession('{"version":1,"browser":null}')).toBeNull();
    expect(parseSession('not json')).toBeNull();
  });
});
