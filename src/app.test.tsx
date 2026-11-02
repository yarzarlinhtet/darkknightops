import { beforeEach, expect, test, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { App } from './app';
import { usePreferences } from './store/preferences';
vi.mock('@/lib/ipc', () => ({
  serversApi: { list: vi.fn().mockResolvedValue([]) },
}));
beforeEach(() => {
  window.location.hash = '/';
  usePreferences.setState({ theme: 'system', sidebarCollapsed: false });
});
test('navigation marks unavailable modules and persists appearance', async () => {
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <App />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('link', { name: 'Projects' }));
  expect(
    await screen.findByText('Not available in Milestone 1'),
  ).toBeInTheDocument();
  await user.click(screen.getByRole('link', { name: 'Settings' }));
  await user.selectOptions(await screen.findByLabelText('Theme'), 'dark');
  expect(document.documentElement).toHaveClass('dark');
  expect(
    JSON.parse(localStorage.getItem('remotedock-preferences')!).state.theme,
  ).toBe('dark');
  await user.click(screen.getByRole('button', { name: 'Collapse sidebar' }));
  expect(usePreferences.getState().sidebarCollapsed).toBe(true);
  await user.selectOptions(screen.getByLabelText('Theme'), 'light');
  expect(document.documentElement).not.toHaveClass('dark');
});

test('system appearance follows OS changes and skip link preserves the route', async () => {
  const media = {
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  };
  vi.mocked(window.matchMedia).mockReturnValueOnce(
    media as unknown as MediaQueryList,
  );
  const user = userEvent.setup();
  const view = render(
    <QueryClientProvider client={new QueryClient()}>
      <App />
    </QueryClientProvider>,
  );
  expect(document.documentElement).not.toHaveClass('dark');
  media.matches = true;
  media.addEventListener.mock.calls[0][1]();
  expect(document.documentElement).toHaveClass('dark');
  await user.click(screen.getByRole('link', { name: 'Skip to content' }));
  expect(document.getElementById('main-content')).toHaveFocus();
  expect(window.location.hash).toBe('#/');
  view.unmount();
  expect(media.removeEventListener).toHaveBeenCalledWith(
    'change',
    media.addEventListener.mock.calls[0][1],
  );
});
