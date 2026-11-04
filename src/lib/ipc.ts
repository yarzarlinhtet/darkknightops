import { invoke, isTauri } from '@tauri-apps/api/core';
import type {
  AppError,
  ErrorCode,
  Server,
  ServerInput,
  SshMetadata,
  ConnectionTestResult,
  ServerInfo,
  ServerMetrics,
  DockerCapabilities,
} from './bindings';
export type { Server, ServerInput, AppError } from './bindings';
export class IpcError extends Error {
  constructor(
    public code: ErrorCode | 'DESKTOP_REQUIRED',
    message: string,
    public field: string | null = null,
  ) {
    super(message);
    this.name = 'IpcError';
  }
}
function normalizeError(error: unknown): IpcError {
  if (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    'message' in error &&
    typeof error.message === 'string'
  ) {
    const e = error as AppError;
    return new IpcError(e.code, e.message, e.field);
  }
  return new IpcError(
    'INTERNAL',
    'The desktop operation failed. Please retry.',
  );
}
type Commands = {
  get_provisioning_preflight: {
    args: { id: string; lease: string };
    result: import('./bindings').Preflight;
  };
  get_docker_service_status: {
    args: { id: string; lease: string };
    result: import('./bindings').Preflight;
  };
  generate_provisioning_plan: {
    args: { id: string; lease: string; grantAccess: boolean };
    result: import('./bindings').ProvisioningPlan;
  };
  start_server_provisioning: {
    args: { id: string; lease: string; planId: string; confirmed: boolean };
    result: import('./bindings').ProvisioningRun;
  };
  grant_docker_access: {
    args: { id: string; lease: string; planId: string; confirmed: boolean };
    result: import('./bindings').ProvisioningRun;
  };
  list_provisioning_runs: {
    args: { id: string };
    result: import('./bindings').ProvisioningRun[];
  };
  get_provisioning_run: {
    args: { id: string; runId: string };
    result: import('./bindings').ProvisioningRun;
  };
  cancel_server_provisioning: {
    args: { id: string; runId: string };
    result: null;
  };

  get_ssh_metadata: { args: { id: string }; result: SshMetadata };
  select_ssh_key: { args: { id: string | null }; result: string | null };
  configure_ssh_credential: {
    args: {
      id: string;
      keyPath: string | null;
      passphrase: string | null;
      savePassphrase: boolean;
    };
    result: null;
  };
  unlock_ssh_key: {
    args: { id: string; lease: string; passphrase: string };
    result: null;
  };
  open_server_session: { args: { id: string }; result: string };
  close_server_session: { args: { id: string; lease: string }; result: null };
  cancel_server_operation: { args: { id: string }; result: null };
  test_server_connection: {
    args: { id: string; lease: string | null };
    result: ConnectionTestResult;
  };
  trust_server_host_key: {
    args: { id: string; observationId: string; replace: boolean };
    result: null;
  };
  get_server_info: { args: { id: string; lease: string }; result: ServerInfo };
  get_docker_capabilities: {
    args: { id: string; lease: string };
    result: DockerCapabilities;
  };
  get_server_metrics: {
    args: { id: string; lease: string };
    result: ServerMetrics;
  };
  list_ssh_keys: { args: undefined; result: import('./bindings').SshKey[] };
  get_ssh_public_key: { args: { id: string }; result: string };
  get_ssh_key: { args: { id: string }; result: import('./bindings').SshKey };
  generate_ssh_key: {
    args: {
      name: string;
      rsa: boolean;
      passphrase: string | null;
      savePassphrase: boolean;
    };
    result: import('./bindings').SshKey;
  };
  import_ssh_key: {
    args: {
      name: string;
      path: string;
      managed: boolean;
      passphrase: string | null;
      savePassphrase: boolean;
    };
    result: import('./bindings').SshKey;
  };
  rename_ssh_key: {
    args: { id: string; name: string };
    result: import('./bindings').SshKey;
  };
  delete_ssh_key: {
    args: { id: string; confirmed: boolean; deleteFile: boolean };
    result: null;
  };
  export_ssh_public_key: { args: { id: string }; result: boolean };
  assign_ssh_key_to_server: {
    args: { id: string; keyId: string | null };
    result: null;
  };
  list_servers_using_ssh_key: { args: { id: string }; result: Server[] };
  get_server_ssh_key_id: { args: { id: string }; result: string | null };
  list_servers: { args: undefined; result: Server[] };
  get_server: { args: { id: string }; result: Server };
  create_server: {
    args: { input: ServerInput; keyId?: string | null };
    result: Server;
  };
  update_server: {
    args: { id: string; input: ServerInput; keyId?: string | null };
    result: Server;
  };
  delete_server: { args: { id: string }; result: null };
};
async function call<K extends keyof Commands>(
  command: K,
  args: Commands[K]['args'],
): Promise<Commands[K]['result']> {
  if (!isTauri())
    throw new IpcError(
      'DESKTOP_REQUIRED',
      'Open the RemoteDock desktop app to manage local servers. Browser preview has no database access.',
    );
  try {
    return await invoke<Commands[K]['result']>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}
export const serversApi = {
  list: () => call('list_servers', undefined),
  get: (id: string) => call('get_server', { id }),
  create: (input: ServerInput, keyId?: string | null) =>
    call('create_server', keyId ? { input, keyId } : { input }),
  update: (id: string, input: ServerInput, keyId?: string | null) =>
    call('update_server', keyId ? { id, input, keyId } : { id, input }),
  delete: (id: string) => call('delete_server', { id }),
};

export const sshApi = {
  metadata: (id: string) => call('get_ssh_metadata', { id }),
  selectKey: (id?: string) => call('select_ssh_key', { id: id ?? null }),
  configure: (
    id: string,
    keyPath: string | null,
    passphrase: string | null,
    savePassphrase: boolean,
  ) =>
    call('configure_ssh_credential', {
      id,
      keyPath,
      passphrase,
      savePassphrase,
    }),
  unlock: (id: string, lease: string, passphrase: string) =>
    call('unlock_ssh_key', { id, lease, passphrase }),
  open: (id: string) => call('open_server_session', { id }),
  close: (id: string, lease: string) =>
    call('close_server_session', { id, lease }),
  cancel: (id: string) => call('cancel_server_operation', { id }),
  test: (id: string, lease: string | null = null) =>
    call('test_server_connection', { id, lease }),
  trust: (id: string, observationId: string, replace: boolean) =>
    call('trust_server_host_key', { id, observationId, replace }),
  info: (id: string, lease: string) => call('get_server_info', { id, lease }),
  docker: (id: string, lease: string) =>
    call('get_docker_capabilities', { id, lease }),
  metrics: (id: string, lease: string) =>
    call('get_server_metrics', { id, lease }),
};

export const keysApi = {
  list: () => call('list_ssh_keys', undefined),
  generate: (
    name: string,
    rsa: boolean,
    passphrase: string | null,
    savePassphrase: boolean,
  ) => call('generate_ssh_key', { name, rsa, passphrase, savePassphrase }),
  import: (
    name: string,
    path: string,
    managed: boolean,
    passphrase: string | null,
    savePassphrase: boolean,
  ) =>
    call('import_ssh_key', { name, path, managed, passphrase, savePassphrase }),
  rename: (id: string, name: string) => call('rename_ssh_key', { id, name }),
  delete: (id: string, deleteFile: boolean) =>
    call('delete_ssh_key', { id, confirmed: true, deleteFile }),
  export: (id: string) => call('export_ssh_public_key', { id }),
  assign: (id: string, keyId: string | null) =>
    call('assign_ssh_key_to_server', { id, keyId }),
  assigned: (id: string) => call('get_server_ssh_key_id', { id }),
  servers: (id: string) => call('list_servers_using_ssh_key', { id }),
};

export const provisioningApi = {
  preflight: (id: string, lease: string) =>
    call('get_provisioning_preflight', { id, lease }),
  plan: (id: string, lease: string, grantAccess = false) =>
    call('generate_provisioning_plan', { id, lease, grantAccess }),
  start: (id: string, lease: string, planId: string, grantAccess: boolean) =>
    call(grantAccess ? 'grant_docker_access' : 'start_server_provisioning', {
      id,
      lease,
      planId,
      confirmed: true,
    }),
  history: (id: string) => call('list_provisioning_runs', { id }),
  get: (id: string, runId: string) =>
    call('get_provisioning_run', { id, runId }),
  cancel: (id: string, runId: string) =>
    call('cancel_server_provisioning', { id, runId }),
};
