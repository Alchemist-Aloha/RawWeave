import type { CheckpointStatus } from '../checkpoint/types';

export interface CheckpointPanelProps {
  status: CheckpointStatus | null;
  loading: boolean;
  onGenerate: () => void | Promise<void>;
  onCancel: () => void | Promise<void>;
  previewActions?: CheckpointPreviewActions;
}

export interface CheckpointPreviewAction {
  onClick: () => void;
  disabled?: boolean;
}

export interface CheckpointPreviewActions {
  input?: CheckpointPreviewAction;
  generated?: CheckpointPreviewAction;
  difference?: CheckpointPreviewAction;
}

const labels: Record<CheckpointStatus['state'], string> = {
  ungenerated: 'Ungenerated',
  generating: 'Generating',
  fresh: 'Fresh',
  stale: 'Stale',
  failed: 'Failed',
  cancelled: 'Cancelled',
};

export function CheckpointPanel({ status, loading, onGenerate, onCancel, previewActions }: CheckpointPanelProps) {
  if (!status) {
    return (
      <section aria-label="Checkpoint" className="checkpoint-panel">
        <span className="eyebrow">Manual checkpoint</span>
        <p className="empty-state empty-state--compact">Checkpoint status unavailable.</p>
      </section>
    );
  }

  const generating = status.state === 'generating';
  const actionLabel = status.committedArtifactId ? 'Regenerate checkpoint' : 'Generate checkpoint';
  return (
    <section aria-label="Checkpoint" className={`checkpoint-panel checkpoint-panel--${status.state}`}>
      <div className="checkpoint-panel__heading">
        <div>
          <span className="eyebrow">Manual checkpoint</span>
          <strong>{labels[status.state]}</strong>
        </div>
        <span className={`checkpoint-state checkpoint-state--${status.state}`}>{labels[status.state]}</span>
      </div>
      <div className="checkpoint-panel__identity">
        <code>{status.nodeId}:{status.outputPort}</code>
        {status.generation && <span>Revision {status.generation.generationRevision}</span>}
      </div>
      {status.state === 'stale' && status.canUseCommitted && (
        <p className="checkpoint-panel__notice">Committed result remains usable while inputs are stale.</p>
      )}
      {generating && (
        <div className="checkpoint-panel__progress" aria-label="Checkpoint progress">
          <span>{status.progress === null ? 'Generating…' : `Generating… ${Math.round(status.progress)}%`}</span>
          <progress max="100" value={status.progress ?? 0}>{status.progress ?? 0}%</progress>
        </div>
      )}
      {status.failure && <p className="checkpoint-panel__error" role="alert">{status.failure}</p>}
      <div className="checkpoint-panel__actions">
        <button
          aria-label={generating ? 'Cancel checkpoint' : actionLabel}
          className={generating ? 'button button--quiet' : 'button button--primary'}
          disabled={loading && !generating}
          onClick={() => void (generating ? onCancel() : onGenerate())}
          type="button"
        >
          {generating ? 'Cancel' : actionLabel.replace(' checkpoint', '')}
        </button>
      </div>
      <details aria-label="Checkpoint provenance" className="checkpoint-panel__details">
        <summary>Inspect stale reason and provenance</summary>
        <dl>
          <div><dt>Current dependency</dt><dd>{status.currentDependencyHash ?? 'Not evaluated'}</dd></div>
          <div><dt>Committed dependency</dt><dd>{status.committedDependencyHash ?? 'None'}</dd></div>
          <div><dt>Artifact</dt><dd>{status.committedArtifactId ?? 'None'}</dd></div>
          {status.provenance && <div><dt>Node version</dt><dd>{status.provenance.nodeVersion}</dd></div>}
        </dl>
        {status.provenance && Object.keys(status.provenance.upstreamHashes).length > 0 && (
          <ul>
            {Object.entries(status.provenance.upstreamHashes).map(([name, hash]) => <li key={name}><code>{name}</code><code>{hash}</code></li>)}
          </ul>
        )}
      </details>
      <div className="checkpoint-panel__previews" aria-label="Checkpoint previews">
        <div>
          <span>Input preview</span>
          <button
            aria-label="Preview checkpoint inputs"
            className="button button--quiet"
            disabled={!previewActions?.input || previewActions.input.disabled}
            onClick={() => void previewActions?.input?.onClick()}
            type="button"
          >
            Open in Viewer
          </button>
        </div>
        <div>
          <span>Generated preview</span>
          <button
            aria-label="Preview committed checkpoint"
            className="button button--quiet"
            disabled={!previewActions?.generated || previewActions.generated.disabled}
            onClick={() => void previewActions?.generated?.onClick()}
            type="button"
          >
            {status.committedArtifactId ? 'Open in Viewer' : 'Not generated'}
          </button>
        </div>
        <div>
          <span>Difference view</span>
          <button
            aria-label="Compare checkpoint previews"
            className="button button--quiet"
            disabled={!previewActions?.difference || previewActions.difference.disabled}
            onClick={() => void previewActions?.difference?.onClick()}
            type="button"
          >
            {status.state === 'stale' ? 'Compare in Viewer' : 'Compare in Viewer'}
          </button>
        </div>
      </div>
    </section>
  );
}
