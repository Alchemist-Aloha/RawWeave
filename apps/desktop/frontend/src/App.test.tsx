import { describe, expect, it } from 'vitest';
import { compatibleDataTypesForNode, describeEditorError } from './App';
import type { EditorNode } from './editor/types';

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