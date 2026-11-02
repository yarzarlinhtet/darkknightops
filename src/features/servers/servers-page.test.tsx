import { beforeEach, expect, test, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ServersPage } from './servers-page';
import { serversApi, IpcError, type Server } from '@/lib/ipc';
vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  serversApi: {
    list: vi.fn(),
    create: vi.fn(),
    update: vi.fn(),
    delete: vi.fn(),
  },
}));
const server: Server = {
  id: 'test-id',
  name: 'Production',
  host: 'example.com',
  port: 22,
  username: 'ubuntu',
  notes: null,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
};
function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ServersPage />
    </QueryClientProvider>,
  );
  return userEvent.setup();
}
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(serversApi.list).mockResolvedValue([server]);
});
test('edit uses existing metadata, keeps failures visible, and refreshes after saving', async () => {
  const user = renderPage();
  await user.click(
    await screen.findByRole('button', { name: 'Edit Production' }),
  );
  const name = screen.getByLabelText('Display name');
  expect(name).toHaveValue('Production');
  await user.clear(name);
  await user.type(name, 'Updated');
  vi.mocked(serversApi.update).mockRejectedValueOnce(
    new IpcError('DATABASE_ERROR', 'Database busy'),
  );
  await user.click(screen.getByRole('button', { name: 'Save changes' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Database busy');
  expect(name).toHaveValue('Updated');
  vi.mocked(serversApi.update).mockResolvedValueOnce({
    ...server,
    name: 'Updated',
  });
  vi.mocked(serversApi.list).mockResolvedValue([
    { ...server, name: 'Updated' },
  ]);
  await user.click(screen.getByRole('button', { name: 'Save changes' }));
  await waitFor(() =>
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument(),
  );
  expect(
    await screen.findByRole('heading', { name: 'Updated' }),
  ).toBeInTheDocument();
  expect(serversApi.update).toHaveBeenCalledWith(
    'test-id',
    expect.objectContaining({ name: 'Updated' }),
  );
});
test('deletion requires confirmation, allows cancellation, and reports failure', async () => {
  const user = renderPage();
  await user.click(
    await screen.findByRole('button', { name: 'Delete Production' }),
  );
  expect(serversApi.delete).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Cancel' }));
  expect(serversApi.delete).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Delete Production' }));
  vi.mocked(serversApi.delete).mockRejectedValueOnce(
    new IpcError('DATABASE_ERROR', 'Cannot delete'),
  );
  await user.click(screen.getByRole('button', { name: 'Delete server' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Cannot delete');
  vi.mocked(serversApi.delete).mockResolvedValueOnce(null);
  vi.mocked(serversApi.list).mockResolvedValue([]);
  await user.click(screen.getByRole('button', { name: 'Delete server' }));
  expect(
    await screen.findByRole('heading', { name: 'No servers yet' }),
  ).toBeInTheDocument();
});
test('invalid forms never call persistence', async () => {
  const user = renderPage();
  await user.click(await screen.findByRole('button', { name: 'Add server' }));
  const dialog = await screen.findByRole('dialog');
  const buttons = dialog.querySelectorAll('button');
  await user.click(
    Array.from(buttons).find((b) => b.textContent === 'Add server')!,
  );
  expect(serversApi.create).not.toHaveBeenCalled();
  expect(screen.getByLabelText('Display name')).toHaveAttribute(
    'aria-invalid',
    'true',
  );
});
test('database errors offer retry without claiming empty inventory', async () => {
  vi.mocked(serversApi.list).mockRejectedValue(
    new IpcError('MIGRATION_FAILED', 'Migration failed'),
  );
  const user = renderPage();
  expect(await screen.findByRole('alert')).toHaveTextContent(
    'Migration failed',
  );
  expect(screen.queryByText('No servers yet')).not.toBeInTheDocument();
  vi.mocked(serversApi.list).mockResolvedValue([]);
  await user.click(screen.getByRole('button', { name: 'Retry' }));
  expect(await screen.findByText('No servers yet')).toBeInTheDocument();
});
