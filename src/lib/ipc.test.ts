import { beforeEach, expect, test, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { serversApi } from './ipc';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
beforeEach(() => {
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke).mockReset();
});
test('browser preview cannot pretend to persist data', async () => {
  vi.mocked(isTauri).mockReturnValue(false);
  await expect(serversApi.list()).rejects.toMatchObject({
    code: 'DESKTOP_REQUIRED',
  });
  expect(invoke).not.toHaveBeenCalled();
});
test('typed wrapper forwards IDs and payloads', async () => {
  const input = {
    name: 'Test',
    host: 'example.com',
    port: 22,
    username: 'ubuntu',
    notes: null,
  };
  await serversApi.create(input);
  expect(invoke).toHaveBeenLastCalledWith('create_server', { input });
  await serversApi.update('id', input);
  expect(invoke).toHaveBeenLastCalledWith('update_server', { id: 'id', input });
  await serversApi.get('id');
  expect(invoke).toHaveBeenLastCalledWith('get_server', { id: 'id' });
  await serversApi.delete('id');
  expect(invoke).toHaveBeenLastCalledWith('delete_server', { id: 'id' });
});
test('structured errors preserve codes and fields without exposing unknown internal errors', async () => {
  vi.mocked(invoke).mockRejectedValue({
    code: 'VALIDATION',
    message: 'Invalid host',
    field: 'host',
  });
  await expect(serversApi.list()).rejects.toMatchObject({
    code: 'VALIDATION',
    message: 'Invalid host',
    field: 'host',
  });
  vi.mocked(invoke).mockRejectedValue('SQL error with private data');
  await expect(serversApi.list()).rejects.toMatchObject({
    code: 'INTERNAL',
    message: 'The desktop operation failed. Please retry.',
  });
});
