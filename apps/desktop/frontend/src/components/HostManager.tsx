import { useCallback, useEffect, useMemo, useState } from 'react';
import { createTauriHostManager } from '../platform/hosts';
import type { ExternalHost, ExternalHostConfig, HostManagerApi } from '../platform/hosts';
import { Icon } from '../ui/Icon';

interface HostManagerProps {
  api?: HostManagerApi;
  onDiscovery?: () => void | Promise<void>;
}

const defaultApi = createTauriHostManager();

function splitList(value: string): string[] {
  return value
    .split(/[\s,]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function replaceHost(hosts: ExternalHost[], updated: ExternalHost): ExternalHost[] {
  const index = hosts.findIndex((host) => host.id === updated.id);
  if (index < 0) return [...hosts, updated];
  return hosts.map((host, hostIndex) => (hostIndex === index ? updated : host));
}

function capabilitySummary(host: ExternalHost): string[] {
  if (!host.capabilities) return [];
  const capabilities = host.capabilities;
  return [
    ...capabilities.pixelFormats,
    capabilities.roi ? 'ROI' : null,
    capabilities.fullFrame ? 'Full frame' : null,
    capabilities.multiInput ? 'Multi-input' : null,
    capabilities.multiOutput ? 'Multi-output' : null,
    capabilities.gpu ? 'GPU' : null,
    capabilities.customUi ? 'Custom UI' : null,
    capabilities.deterministic ? 'Deterministic' : null,
    capabilities.dataPlane ? 'Data plane' : null,
  ].filter((value): value is string => value !== null);
}

export function HostManager({ api = defaultApi, onDiscovery }: HostManagerProps) {
  const [hosts, setHosts] = useState<ExternalHost[]>([]);
  const [hostId, setHostId] = useState('');
  const [executable, setExecutable] = useState('');
  const [argumentsText, setArgumentsText] = useState('');
  const [environmentText, setEnvironmentText] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadHosts = useCallback(async () => {
    setLoading(true);
    try {
      setHosts(await api.list());
      setError(null);
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => {
    void loadHosts();
  }, [loadHosts]);

  const runHostAction = useCallback(
    async (key: string, action: () => Promise<ExternalHost>, refreshDescriptors = false) => {
      setBusy(key);
      try {
        const updated = await action();
        setHosts((current) => replaceHost(current, updated));
        setError(null);
        if (refreshDescriptors) await onDiscovery?.();
      } catch (actionError) {
        setError(errorMessage(actionError));
      } finally {
        setBusy(null);
      }
    },
    [onDiscovery],
  );

  const discoverAll = useCallback(async () => {
    setBusy('refresh');
    try {
      setHosts(await api.discoverAll());
      setError(null);
      await onDiscovery?.();
    } catch (discoverError) {
      setError(errorMessage(discoverError));
    } finally {
      setBusy(null);
    }
  }, [api, onDiscovery]);

  const removeHost = useCallback(
    async (id: string) => {
      setBusy(`remove:${id}`);
      try {
        await api.remove(id);
        setHosts((current) => current.filter((host) => host.id !== id));
        setError(null);
      } catch (removeError) {
        setError(errorMessage(removeError));
      } finally {
        setBusy(null);
      }
    },
    [api],
  );

  const addHost = useCallback(
    async (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      const config: ExternalHostConfig = {
        id: hostId.trim(),
        executable: executable.trim(),
        args: splitList(argumentsText),
        envAllowlist: splitList(environmentText),
        environment: {},
      };
      setBusy('add');
      try {
        const added = await api.add(config);
        setHosts((current) => replaceHost(current, added));
        setHostId('');
        setExecutable('');
        setArgumentsText('');
        setEnvironmentText('');
        setError(null);
      } catch (addError) {
        setError(errorMessage(addError));
      } finally {
        setBusy(null);
      }
    },
    [api, argumentsText, environmentText, executable, hostId],
  );

  const hostCountLabel = useMemo(() => `${hosts.length} configured`, [hosts.length]);

  return (
    <section aria-label="External hosts" className="host-manager">
      <div className="host-manager__heading">
        <div>
          <span className="eyebrow">Integrations</span>
          <h2>External hosts</h2>
          <small>{hostCountLabel}</small>
        </div>
        <button
          aria-label="Refresh external hosts"
          className="button button--quiet"
          disabled={busy === 'refresh'}
          onClick={() => void discoverAll()}
          type="button"
        >
          {busy === 'refresh' ? 'Discovering…' : 'Refresh'}
        </button>
      </div>

      {error && (
        <div className="host-manager__error" role="alert">
          <span>{error}</span>
          <button aria-label="Dismiss host manager error" onClick={() => setError(null)} type="button"><Icon name="close" /></button>
        </div>
      )}

      <div className="host-manager__hosts">
        {loading && <p className="empty-state">Loading external hosts…</p>}
        {!loading && hosts.length === 0 && <p className="empty-state">No external hosts configured.</p>}
        {hosts.map((host) => {
          const capabilities = capabilitySummary(host);
          return (
            <article aria-label={`External host ${host.id}`} className="host-card" key={host.id}>
              <div className="host-card__heading">
                <div>
                  <strong>{host.id}</strong>
                  <small>{host.executable}</small>
                </div>
                <span className={`host-status host-status--${host.status}`}>{host.status}</span>
              </div>
              <dl className="host-card__details">
                <div>
                  <dt>Protocol</dt>
                  <dd>
                    Protocol {host.protocolVersion
                      ? `${host.protocolVersion.major}.${host.protocolVersion.minor}`
                      : 'Unavailable'}
                  </dd>
                </div>
                <div>
                  <dt>Nodes</dt>
                  <dd>{host.nodes.length}</dd>
                </div>
                <div>
                  <dt>Thread safety</dt>
                  <dd>{host.capabilities?.threadSafety ?? 'Unavailable'}</dd>
                </div>
              </dl>
              {capabilities.length > 0 && (
                <div className="host-card__capabilities">
                  {capabilities.map((capability) => <span key={capability}>{capability}</span>)}
                </div>
              )}
              {host.nodes.length > 0 && (
                <ul className="host-card__nodes">
                  {host.nodes.map((node) => <li key={node.typeId}>{node.name} <code>{node.typeId}</code></li>)}
                </ul>
              )}
              {host.error && <p className="host-card__error">{host.error}</p>}
              <div className="host-card__actions">
                <button
                  aria-label={`Test host ${host.id}`}
                  className="button button--small"
                  disabled={busy !== null}
                  onClick={() => void runHostAction(`test:${host.id}`, () => api.test(host.id))}
                  type="button"
                >
                  Test
                </button>
                <button
                  aria-label={`Discover host ${host.id}`}
                  className="button button--small"
                  disabled={busy !== null}
                  onClick={() => void runHostAction(`discover:${host.id}`, () => api.discover(host.id), true)}
                  type="button"
                >
                  Discover
                </button>
                <button
                  aria-label={`Remove host ${host.id}`}
                  className="button button--small button--danger"
                  disabled={busy !== null}
                  onClick={() => void removeHost(host.id)}
                  type="button"
                >
                  Remove
                </button>
              </div>
            </article>
          );
        })}
      </div>

      <form className="host-manager__form" onSubmit={addHost}>
        <div className="host-manager__form-heading">
          <div>
            <span className="eyebrow">Configure</span>
            <strong>Add external host</strong>
          </div>
          <small>Arguments and environment names are whitespace-separated.</small>
        </div>
        <div className="host-manager__fields">
          <label>Host id<input aria-label="Host id" required value={hostId} onChange={(event) => setHostId(event.target.value)} /></label>
          <label>Host executable<input aria-label="Host executable" required value={executable} onChange={(event) => setExecutable(event.target.value)} /></label>
          <label>Host arguments<input aria-label="Host arguments" value={argumentsText} onChange={(event) => setArgumentsText(event.target.value)} /></label>
          <label>Environment allowlist<input aria-label="Environment allowlist" value={environmentText} onChange={(event) => setEnvironmentText(event.target.value)} /></label>
        </div>
        <button className="button button--primary" disabled={busy !== null} type="submit">
          {busy === 'add' ? 'Adding…' : 'Add host'}
        </button>
      </form>
    </section>
  );
}
