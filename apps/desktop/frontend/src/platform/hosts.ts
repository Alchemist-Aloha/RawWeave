import { invoke } from '@tauri-apps/api/core';

export interface HostProtocolVersion {
  major: number;
  minor: number;
}

export interface HostCapabilities {
  pixelFormats: string[];
  roi: boolean;
  fullFrame: boolean;
  multiInput: boolean;
  multiOutput: boolean;
  threadSafety: string;
  gpu: boolean;
  customUi: boolean;
  deterministic: boolean;
  dataPlane: boolean;
}

export interface HostNode {
  typeId: string;
  name: string;
  version: number;
}

export interface ExternalHost {
  id: string;
  executable: string;
  args: string[];
  envAllowlist: string[];
  status: string;
  protocolVersion: HostProtocolVersion | null;
  capabilities: HostCapabilities | null;
  nodes: HostNode[];
  error: string | null;
}

export interface ExternalHostConfig {
  id: string;
  executable: string;
  args: string[];
  envAllowlist: string[];
  environment: Record<string, string>;
}

export interface ExternalHostDiagnostics {
  hostId: string;
  executable: string;
  status: string;
  protocolVersion: HostProtocolVersion | null;
  capabilities: HostCapabilities | null;
  nodeCount: number;
  error: string | null;
}

export interface HostManagerApi {
  list(): Promise<ExternalHost[]>;
  add(config: ExternalHostConfig): Promise<ExternalHost>;
  remove(hostId: string): Promise<void>;
  test(hostId: string): Promise<ExternalHost>;
  discover(hostId: string): Promise<ExternalHost>;
  discoverAll(): Promise<ExternalHost[]>;
  diagnostics(hostId: string): Promise<ExternalHostDiagnostics>;
}

function message(error: unknown): Error {
  if (error instanceof Error) return error;
  if (typeof error === 'string') return new Error(error);
  try {
    return new Error(JSON.stringify(error));
  } catch {
    return new Error(String(error));
  }
}

export function createTauriHostManager(): HostManagerApi {
  return {
    async list() {
      try {
        return await invoke<ExternalHost[]>('list_external_hosts');
      } catch (error) {
        throw message(error);
      }
    },
    async add(config) {
      try {
        return await invoke<ExternalHost>('add_external_host', { config });
      } catch (error) {
        throw message(error);
      }
    },
    async remove(hostId) {
      try {
        await invoke('remove_external_host', { hostId });
      } catch (error) {
        throw message(error);
      }
    },
    async test(hostId) {
      try {
        return await invoke<ExternalHost>('test_external_host', { hostId });
      } catch (error) {
        throw message(error);
      }
    },
    async discover(hostId) {
      try {
        return await invoke<ExternalHost>('discover_external_host', { hostId });
      } catch (error) {
        throw message(error);
      }
    },
    async discoverAll() {
      try {
        return await invoke<ExternalHost[]>('discover_external_hosts');
      } catch (error) {
        throw message(error);
      }
    },
    async diagnostics(hostId) {
      try {
        return await invoke<ExternalHostDiagnostics>('external_host_diagnostics', { hostId });
      } catch (error) {
        throw message(error);
      }
    },
  };
}
