import { Component, type ErrorInfo, type ReactNode } from 'react';

interface AppErrorBoundaryProps {
  children: ReactNode;
}

interface AppErrorBoundaryState {
  error: Error | null;
}

/**
 * A render error would otherwise unmount the whole tree and leave a blank
 * window with no explanation. Show the failure and a way back instead.
 */
export class AppErrorBoundary extends Component<AppErrorBoundaryProps, AppErrorBoundaryState> {
  public state: AppErrorBoundaryState = { error: null };

  public static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error };
  }

  public componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('RawWeave editor crashed', error, info.componentStack);
  }

  public render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className="crash-screen" role="alert">
        <span className="eyebrow">Editor error</span>
        <h1>RawWeave hit an unexpected error</h1>
        <p>The editor stopped to avoid showing an empty window. Details:</p>
        <pre>{error.message}</pre>
        <div className="crash-screen__actions">
          <button className="button button--primary" onClick={() => this.setState({ error: null })} type="button">
            Try again
          </button>
          <button className="button button--quiet" onClick={() => window.location.reload()} type="button">
            Reload
          </button>
        </div>
      </div>
    );
  }
}
