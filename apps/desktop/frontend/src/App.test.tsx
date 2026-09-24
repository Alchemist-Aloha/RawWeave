import { describe, expect, it } from 'vitest';
import { compatibleDataTypesForNode, describeEditorError, restoreBrowserSession, selectionFromNodeChanges } from './App';
import type { EditorNode, OpenImageResult } from './editor/types';
import { defaultSession } from './browser/session';
import type { NodeChange } from '@xyflow/react';

describe('canvas selection changes', () => {
  it('applies React Flow select changes to the editor selection', () => {
    const changes = [
      { id: 'b', type: 'select', selected: true },
      { id: 'a', type: 'select', selected: false },
    ] as NodeChange[];

    expect(selectionFromNodeChanges(['a'], changes)).toEqual(['b']);
  });

  it('returns null for redundant selection changes so no publish loops', () => {
    const selecting = [{ id: 'a', type: 'select', selected: true }] as NodeChange[];
    expect(selectionFromNodeChanges(['a'], selecting)).toBeNull();

    const deselectingMissing = [{ id: 'z', type: 'select', selected: false }] as NodeChange[];
    expect(selectionFromNodeChanges(['a'], deselectingMissing)).toBeNull();
  });

  it('ignores non-selection changes', () => {
    const changes = [{ id: 'a', type: 'position', position: { x: 1, y: 2 } }] as NodeChange[];
    expect(selectionFromNodeChanges(['a'], changes)).toBeNull();
  });
});

describe('editor error UX', () => {
  it('redacts secret-shaped details and gives contextual recovery guidance', () => {
    const notice = describeEditorError(
      'AI provider failed for Exposure: api_key=sk-secret-value',
      { kind: 'editor', nodeLabel: 'Exposure (exposure)', dependencyIssue: true },
    );

    expect(notice.title).toBe('Editor operation failed');
    expect(notice.message).toContain('Exposure (exposure)');
    expect(notice.message).not.toContain('sk-secret-value');
    expect(notice.guidance).toMatch(/dependency|provider|retry/i);
  });

  it('explains how to retry a failed image source operation', () => {
    const notice = describeEditorError('permission denied', { kind: 'image' });

    expect(notice.title).toBe('Image source could not be opened');
    expect(notice.guidance).toMatch(/retry|supported|permission/i);
  });

  it('restores the graph before opening its source and attaching viewer targets', async () => {
    const session = defaultSession('/photos');
    session.queue.currentPath = '/photos/current.dng';
    session.workflow.unsavedWorkingCopy = '{"version":1,"graph":"saved-graph"}';
    session.viewer.targets.A = { nodeId: 'display', outputPort: 'display' };
    session.panelLayout = 'split';
    const events: string[] = [];
    const source: OpenImageResult = {
      kind: 'raw',
      width: 400,
      height: 300,
      revision: 7,
      metadata: null,
    };

    await restoreBrowserSession(session, {
      initializeEditor: async () => { events.push('initialize'); },
      setImageSets: () => { events.push('image-sets'); },
      setActiveImageSetId: () => { events.push('active-image-set'); },
      setPanelLayout: () => { events.push('panel-layout'); },
      loadWorkflow: async () => { events.push('load-workflow'); },
      openImage: async (path) => {
        events.push(`open-image:${path}`);
        return source;
      },
      setSourceDimensions: () => { events.push('source-dimensions'); },
      setViewerTargets: (targets) => {
        events.push(`viewer-targets:${targets.A?.nodeId ?? 'none'}`);
      },
    });

    expect(events).toEqual([
      'initialize',
      'image-sets',
      'active-image-set',
      'panel-layout',
      'load-workflow',
      'open-image:/photos/current.dng',
      'source-dimensions',
      'viewer-targets:display',
    ]);
  });

  it('derives compatible library types from the selected node outputs', () => {
    const node = {
      id: 'display-1',
      typeId: 'core.display',
      parameters: {},
      position: { x: 0, y: 0 },
      descriptor: {
        typeId: 'core.display',
        name: 'Display',
        version: 1,
        inputs: [],
        outputs: [
          { id: 'display', name: 'Display', dataType: 'color.DisplayRGB', required: false },
          { id: 'mask', name: 'Mask', dataType: 'core.Mask', required: false },
          { id: 'display-duplicate', name: 'Display 2', dataType: 'color.DisplayRGB', required: false },
        ],
        parameters: [],
      },
    } satisfies EditorNode;

    expect(compatibleDataTypesForNode(node)).toEqual(['color.DisplayRGB', 'core.Mask']);
    expect(compatibleDataTypesForNode(undefined)).toEqual([]);
  });
});