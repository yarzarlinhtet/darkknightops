import { StrictMode } from 'react';
import { beforeEach, expect, test, vi } from 'vitest';
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { ServerDetailPage } from './server-detail';
import { serversApi, sshApi } from '@/lib/ipc';
import type { SshMetadata } from '@/lib/bindings';
vi.mock('@/lib/ipc', () => ({
  keysApi: {
    list: vi.fn().mockResolvedValue([]),
    assigned: vi.fn().mockResolvedValue(null),
    assign: vi.fn().mockResolvedValue(null),
  },
  serversApi: { get: vi.fn() },
  sshApi: Object.fromEntries(
    [
      'metadata',
      'open',
      'close',
      'test',
      'trust',
      'info',
      'docker',
      'metrics',
      'cancel',
      'selectKey',
      'configure',
      'unlock',
    ].map((key) => [key, vi.fn()]),
  ),
}));
const metadata: SshMetadata = {
  keyPath: '/tmp/test-key',
  passphraseSaved: false,
  trustedFingerprint: null,
  trustedAlgorithm: null,
  lastTest: null,
  lastSuccessAt: null,
  info: null,
  docker: null,
};
function page() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <StrictMode>
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={['/servers/test']}>
          <Routes>
            <Route path="/servers/:id" element={<ServerDetailPage />} />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>
    </StrictMode>,
  );
}
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(serversApi.get).mockResolvedValue({
    id: 'test',
    name: 'Fixture',
    host: '127.0.0.1',
    port: 22,
    username: 'tester',
    notes: null,
    createdAt: '',
    updatedAt: '',
  });
  vi.mocked(sshApi.metadata).mockResolvedValue(metadata);
  vi.mocked(sshApi.open).mockResolvedValue('lease');
  vi.mocked(sshApi.close).mockResolvedValue(null);
  vi.mocked(sshApi.cancel).mockResolvedValue(null);
  vi.mocked(sshApi.test).mockResolvedValue({
    status: 'CONNECTED',
    message: 'Verified',
    latencyMs: 12,
    errorCode: null,
    testedAt: new Date().toISOString(),
    observation: null,
  });
  vi.mocked(sshApi.metrics).mockResolvedValue({
    cpuPercent: 25,
    memoryPercent: 50,
    diskPercent: 75,
    uptimeSeconds: 100,
    dockerAvailable: false,
    sampledAt: new Date().toISOString(),
  });
});
test('historical connection does not start polling; real test discovers and cleanup closes the lease', async () => {
  vi.mocked(sshApi.metadata).mockResolvedValue({
    ...metadata,
    lastTest: {
      status: 'CONNECTED',
      message: 'Old',
      latencyMs: 10,
      errorCode: null,
      testedAt: '2020-01-01T00:00:00Z',
      observation: null,
    },
  });
  const view = page();
  await waitFor(() =>
    expect(
      screen.getByRole('button', { name: 'Test Connection' }),
    ).toBeEnabled(),
  );
  expect(sshApi.metrics).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Test Connection' }));
  await screen.findByText('25.0%');
  expect(sshApi.info).toHaveBeenCalledWith('test', 'lease');
  expect(sshApi.docker).toHaveBeenCalledWith('test', 'lease');
  view.unmount();
  await waitFor(() =>
    expect(sshApi.close).toHaveBeenCalledWith('test', 'lease'),
  );
});
test('unknown key approval is explicit and bound to the backend observation', async () => {
  vi.mocked(sshApi.test).mockResolvedValueOnce({
    status: 'HOST_KEY_UNTRUSTED',
    message: 'Review key',
    latencyMs: null,
    errorCode: 'HOST_KEY_UNTRUSTED',
    testedAt: new Date().toISOString(),
    observation: {
      id: 'exact-observation',
      host: '127.0.0.1',
      port: 22,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:fixture',
      previousFingerprint: null,
    },
  });
  const view = page();
  await waitFor(() =>
    expect(
      screen.getByRole('button', { name: 'Test Connection' }),
    ).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Test Connection' }));
  await screen.findByRole('dialog');
  expect(sshApi.trust).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Trust and reconnect' }));
  await waitFor(() =>
    expect(sshApi.trust).toHaveBeenCalledWith(
      'test',
      'exact-observation',
      false,
    ),
  );
  view.unmount();
});
test('temporary passphrase is cleared from the input and mutation variables', async () => {
  const view = page();
  await screen.findByLabelText('Key passphrase (optional)');
  fireEvent.change(screen.getByLabelText('Key passphrase (optional)'), {
    target: { value: 'temporary-secret' },
  });
  fireEvent.click(
    screen.getByRole('button', { name: 'Unlock for this session' }),
  );
  await waitFor(() =>
    expect(sshApi.unlock).toHaveBeenCalledWith(
      'test',
      'lease',
      'temporary-secret',
    ),
  );
  expect(screen.getByLabelText('Key passphrase (optional)')).toHaveValue('');
  view.unmount();
});
test('polling does not overlap and stops on unmount', async () => {
  vi.useFakeTimers();
  try {
    let finish:
      ((value: Awaited<ReturnType<typeof sshApi.metrics>>) => void) | undefined;
    vi.mocked(sshApi.metrics).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const view = page();
    for (let n = 0; n < 8; n++)
      await act(async () => {
        await vi.advanceTimersByTimeAsync(50);
      });
    fireEvent.click(screen.getByRole('button', { name: 'Test Connection' }));
    for (let n = 0; n < 8; n++)
      await act(async () => {
        await vi.advanceTimersByTimeAsync(50);
      });
    expect(sshApi.metrics).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30000);
    });
    expect(sshApi.metrics).toHaveBeenCalledTimes(1);
    await act(async () => {
      finish?.({
        cpuPercent: 10,
        memoryPercent: 20,
        diskPercent: 30,
        uptimeSeconds: 10,
        dockerAvailable: false,
        sampledAt: new Date().toISOString(),
      });
      await vi.advanceTimersByTimeAsync(50);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10000);
    });
    expect(sshApi.metrics).toHaveBeenCalledTimes(2);
    view.unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30000);
    });
    expect(sshApi.metrics).toHaveBeenCalledTimes(2);
  } finally {
    vi.useRealTimers();
  }
});
