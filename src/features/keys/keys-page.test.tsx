import { expect, test, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { KeyDialog, SshKeysPage, KeySelector } from './keys-page';
import { keysApi } from '@/lib/ipc';
import type { SshKey } from '@/lib/bindings';
vi.mock('@/lib/ipc', () => ({
  keysApi: {
    list: vi.fn(),
    generate: vi.fn(),
    import: vi.fn(),
    servers: vi.fn(),
    delete: vi.fn(),
    rename: vi.fn(),
    export: vi.fn(),
  },
  sshApi: { selectKey: vi.fn() },
}));
const key: SshKey = {
  id: 'key-id',
  name: 'Shared key',
  algorithm: 'ED25519',
  fingerprintSha256: 'SHA256:example',
  publicKey: 'ssh-ed25519 AAAA',
  storageType: 'MANAGED',
  createdAt: '2026-01-01',
  updatedAt: '2026-01-01',
  serverCount: 0,
  passphraseSaved: false,
};
function mount(component: React.ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <MemoryRouter>
      <QueryClientProvider client={client}>{component}</QueryClientProvider>
    </MemoryRouter>,
  );
  return client;
}
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(keysApi.list).mockResolvedValue([key]);
  vi.mocked(keysApi.servers).mockResolvedValue([]);
});
test('generation sends a transient secret directly and leaves no cached secret', async () => {
  const user = userEvent.setup();
  vi.mocked(keysApi.generate).mockResolvedValue(key);
  const created = vi.fn();
  const client = mount(
    <KeyDialog mode="generate" onClose={() => {}} onCreated={created} />,
  );
  await user.type(screen.getByLabelText('Key name'), 'Shared key');
  await user.type(
    screen.getByLabelText('Passphrase (optional)'),
    'test-secret',
  );
  await user.type(screen.getByLabelText('Confirm passphrase'), 'test-secret');
  await user.click(screen.getByRole('button', { name: 'Generate key' }));
  await waitFor(() => expect(created).toHaveBeenCalledWith(key));
  expect(keysApi.generate).toHaveBeenCalledWith(
    'Shared key',
    false,
    'test-secret',
    false,
  );
  expect(screen.getByLabelText('Passphrase (optional)')).toHaveValue('');
  expect(client.getMutationCache().getAll()).toHaveLength(0);
  expect(JSON.stringify(client.getQueryCache().getAll())).not.toContain(
    'test-secret',
  );
});
test('assigned keys cannot be deleted in details', async () => {
  vi.mocked(keysApi.list).mockResolvedValue([{ ...key, serverCount: 2 }]);
  const user = userEvent.setup();
  mount(<SshKeysPage />);
  await user.click(await screen.findByRole('button', { name: key.name }));
  expect(
    await screen.findByText(
      'Unassign this key from all servers before deleting it.',
    ),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole('button', { name: 'Delete key…' }),
  ).not.toBeInTheDocument();
  expect(keysApi.delete).not.toHaveBeenCalled();
});
test('managed deletion requires confirmation and file deletion defaults off', async () => {
  const user = userEvent.setup();
  mount(<SshKeysPage />);
  await user.click(await screen.findByRole('button', { name: key.name }));
  await user.click(screen.getByRole('button', { name: 'Delete key…' }));
  expect(keysApi.delete).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Confirm removal' }));
  await waitFor(() =>
    expect(keysApi.delete).toHaveBeenCalledWith(key.id, false),
  );
});
test('selector exposes reusable identity', async () => {
  const change = vi.fn();
  const user = userEvent.setup();
  mount(<KeySelector value={null} onChange={change} />);
  await screen.findByRole('option', { name: /Shared key/ });
  await user.selectOptions(screen.getByRole('combobox'), key.id);
  expect(change).toHaveBeenCalledWith(key.id);
});
