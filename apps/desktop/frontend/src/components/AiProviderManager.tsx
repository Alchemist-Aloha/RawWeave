import { useCallback, useEffect, useState } from 'react';
import type {
  AiProvider,
  AiProviderConfig,
  AiProviderPlatform,
} from '../platform/ai-provider-types';
import { createAiProviderPlatform } from '../platform/ai-provider';
import { Icon } from '../ui/Icon';

interface AiProviderManagerProps {
  api?: AiProviderPlatform;
}

const defaultApi = createAiProviderPlatform();

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function replaceProvider(providers: AiProvider[], updated: AiProvider): AiProvider[] {
  const index = providers.findIndex((provider) => provider.id === updated.id);
  if (index < 0) return [...providers, updated];
  return providers.map((provider, providerIndex) => (providerIndex === index ? updated : provider));
}

export function AiProviderManager({ api = defaultApi }: AiProviderManagerProps) {
  const [providers, setProviders] = useState<AiProvider[]>([]);
  const [id, setId] = useState('local-comfy');
  const [name, setName] = useState('Local ComfyUI');
  const [kind, setKind] = useState<AiProviderConfig['kind']>('comfy_ui');
  const [baseUrl, setBaseUrl] = useState('http://127.0.0.1:8188');
  const [clientId, setClientId] = useState('rawweave');
  const [manifestText, setManifestText] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadProviders = useCallback(async () => {
    setLoading(true);
    try {
      setProviders(await api.list());
      setError(null);
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => {
    void loadProviders();
  }, [loadProviders]);

  const testProvider = useCallback(async (providerId: string) => {
    setBusy(`test:${providerId}`);
    try {
      const updated = await api.test(providerId);
      setProviders((current) => replaceProvider(current, updated));
      setError(null);
    } catch (testError) {
      setError(errorMessage(testError));
    } finally {
      setBusy(null);
    }
  }, [api]);

  const removeProvider = useCallback(async (providerId: string) => {
    setBusy(`remove:${providerId}`);
    try {
      await api.remove(providerId);
      setProviders((current) => current.filter((provider) => provider.id !== providerId));
      setError(null);
    } catch (removeError) {
      setError(errorMessage(removeError));
    } finally {
      setBusy(null);
    }
  }, [api]);

  const addProvider = useCallback(async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    let manifest: unknown;
    if (manifestText.trim()) {
      try {
        manifest = JSON.parse(manifestText);
      } catch {
        setError('HTTP provider manifest must be valid JSON');
        return;
      }
    }
    const config: AiProviderConfig = {
      id: id.trim(),
      name: name.trim(),
      kind,
      baseUrl: baseUrl.trim(),
      clientId: clientId.trim() || undefined,
      manifest,
    };
    setBusy('add');
    try {
      const added = await api.add(config);
      setProviders((current) => replaceProvider(current, added));
      setError(null);
    } catch (addError) {
      setError(errorMessage(addError));
    } finally {
      setBusy(null);
    }
  }, [api, baseUrl, clientId, id, kind, manifestText, name]);

  return (
    <section aria-label="AI providers" className="ai-provider-manager">
      <div className="ai-provider-manager__heading">
        <div>
          <span className="eyebrow">AI execution</span>
          <h2>AI providers</h2>
          <small>{providers.length} configured</small>
        </div>
        <button
          aria-label="Refresh AI providers"
          className="button button--quiet"
          disabled={busy !== null}
          onClick={() => void loadProviders()}
          type="button"
        >
          Refresh
        </button>
      </div>
      {error && (
        <div className="ai-provider-manager__error" role="alert">
          <span>{error}</span>
          <button aria-label="Dismiss AI provider error" onClick={() => setError(null)} type="button"><Icon name="close" /></button>
        </div>
      )}
      <div className="ai-provider-manager__providers">
        {loading && <p className="empty-state">Loading AI providers…</p>}
        {!loading && providers.length === 0 && <p className="empty-state">No AI providers configured.</p>}
        {providers.map((provider) => (
          <article aria-label={`AI provider ${provider.id}`} className="ai-provider-card" key={provider.id}>
            <div className="ai-provider-card__heading">
              <div>
                <strong>{provider.name}</strong>
                <small>{provider.id} · {provider.kind}</small>
              </div>
              <span className={`ai-provider-status ai-provider-status--${provider.status}`}>{provider.status}</span>
            </div>
            <code>{provider.baseUrl}</code>
            {provider.error && <p className="ai-provider-card__error">{provider.error}</p>}
            <div className="ai-provider-card__actions">
              <button
                aria-label={`Test AI provider ${provider.id}`}
                className="button button--small"
                disabled={busy !== null}
                onClick={() => void testProvider(provider.id)}
                type="button"
              >
                {busy === `test:${provider.id}` ? 'Testing…' : 'Test'}
              </button>
              <button
                aria-label={`Remove AI provider ${provider.id}`}
                className="button button--small button--danger"
                disabled={busy !== null}
                onClick={() => void removeProvider(provider.id)}
                type="button"
              >
                Remove
              </button>
            </div>
          </article>
        ))}
      </div>
      <form className="ai-provider-manager__form" onSubmit={addProvider}>
        <div className="ai-provider-manager__form-heading">
          <div>
            <span className="eyebrow">Configure</span>
            <strong>Add AI provider</strong>
          </div>
          <small>Secrets stay in secure references; only provider configuration is persisted.</small>
        </div>
        <div className="ai-provider-manager__fields">
          <label>Provider id<input aria-label="AI provider id" required value={id} onChange={(event) => setId(event.target.value)} /></label>
          <label>Name<input aria-label="AI provider name" required value={name} onChange={(event) => setName(event.target.value)} /></label>
          <label>Type
            <select aria-label="AI provider type" value={kind} onChange={(event) => setKind(event.target.value as AiProviderConfig['kind'])}>
              <option value="comfy_ui">ComfyUI</option>
              <option value="http">HTTP manifest</option>
            </select>
          </label>
          <label>Base URL<input aria-label="AI provider base URL" required type="url" value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} /></label>
          <label>Client id<input aria-label="AI provider client id" value={clientId} onChange={(event) => setClientId(event.target.value)} /></label>
          {kind === 'http' && <label>Manifest JSON<textarea aria-label="AI provider manifest" value={manifestText} onChange={(event) => setManifestText(event.target.value)} /></label>}
        </div>
        <button className="button button--primary" disabled={busy !== null} type="submit">
          {busy === 'add' ? 'Adding…' : 'Add provider'}
        </button>
      </form>
    </section>
  );
}
