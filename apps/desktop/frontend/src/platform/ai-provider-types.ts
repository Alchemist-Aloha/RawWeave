export type AiProviderKind = 'comfy_ui' | 'http';
export type AiProviderStatus = 'configured' | 'ready' | 'error';

export interface AiProviderCapabilities {
  operations: string[];
  supportsCancel: boolean;
  supportsProgress: boolean;
  maxImageBytes: number;
  colorInterchange: {
    format: string;
    colorSpace: string;
    alphaMode: string;
    lossless: boolean;
  };
}

export interface AiProvider {
  id: string;
  name: string;
  kind: AiProviderKind;
  baseUrl: string;
  clientId?: string;
  status: AiProviderStatus;
  capabilities?: AiProviderCapabilities | null;
  error?: string | null;
}

export interface AiProviderConfig {
  id: string;
  name: string;
  kind: AiProviderKind;
  baseUrl: string;
  clientId?: string;
  manifest?: unknown;
}

export interface AiProviderPlatform {
  list(): Promise<AiProvider[]>;
  add(config: AiProviderConfig): Promise<AiProvider>;
  remove(providerId: string): Promise<void>;
  test(providerId: string): Promise<AiProvider>;
}
