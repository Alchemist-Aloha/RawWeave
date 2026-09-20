import { describe, expect, it } from 'vitest';
import { describeEditorError } from './App';

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
});