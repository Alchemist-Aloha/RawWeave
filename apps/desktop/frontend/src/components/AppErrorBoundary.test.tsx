import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AppErrorBoundary } from './AppErrorBoundary';

function Boom(): never {
  throw new Error('kaboom from a node panel');
}

describe('AppErrorBoundary', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('shows the failure instead of a blank window', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => root.render(
      <AppErrorBoundary>
        <Boom />
      </AppErrorBoundary>,
    ));

    const alert = host.querySelector('[role="alert"]');
    expect(alert).not.toBeNull();
    expect(alert?.textContent).toContain('unexpected error');
    expect(alert?.textContent).toContain('kaboom from a node panel');
    expect(alert?.querySelector('button')?.textContent).toBe('Try again');

    await act(async () => root.unmount());
    host.remove();
  });
});
