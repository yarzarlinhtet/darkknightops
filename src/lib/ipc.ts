import { invoke, isTauri } from '@tauri-apps/api/core';
import type { AppError, ErrorCode, Server, ServerInput } from './bindings';
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
  list_servers: { args: undefined; result: Server[] };
  get_server: { args: { id: string }; result: Server };
  create_server: { args: { input: ServerInput }; result: Server };
  update_server: { args: { id: string; input: ServerInput }; result: Server };
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
  create: (input: ServerInput) => call('create_server', { input }),
  update: (id: string, input: ServerInput) =>
    call('update_server', { id, input }),
  delete: (id: string) => call('delete_server', { id }),
};
