import { describe, expect, it } from 'vitest';
import { describeOperationError } from './errors';

describe('operation error descriptors', () => {
  it('adds checkpoint node/output context, retry guidance, and redacts secrets', () => {
    const notice = describeOperationError(
      'provider rejected api_key=«redacted:sk-…»',
      {
        operation: 'checkpoint-generate',
        nodeId: 'checkpoint-1',
        nodeLabel: 'Manual checkpoint',
        outputPort: 'image',
        dependencyIssue: true,
      },
    );

    expect(notice.title).toBe('Checkpoint generation failed');
    expect(notice.message).toContain('Manual checkpoint');
    expect(notice.message).toContain('checkpoint-1');
    expect(notice.message).toContain('image');
    expect(notice.message).not.toContain('«redacted:sk-…»');
    expect(notice.guidance).toMatch(/dependency|provider/i);
    expect(notice.retryLabel).toMatch(/retry/i);
  });

  it('describes viewer render failures with node context and a retry action', () => {
    const notice = describeOperationError('render unavailable', {
      operation: 'viewer-render',
      nodeId: 'exposure-2',
      nodeLabel: 'Exposure',
      outputPort: 'display',
      imageLabel: 'IMG_0001.nef',
    });

    expect(notice.title).toBe('Viewer render failed');
    expect(notice.message).toMatch(/Exposure|exposure-2|display|IMG_0001.nef/);
    expect(notice.guidance).toMatch(/retry|provider|dependency/i);
    expect(notice.retryLabel).toBe('Retry render');
  });
});
