import { beforeEach, expect, test, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ProvisioningPanel } from './provisioning-panel';
import { provisioningApi } from '@/lib/ipc';
import type {
  Preflight,
  ProvisioningPlan,
  ProvisioningRun,
} from '@/lib/bindings';
vi.mock('@/lib/ipc', () => ({
  provisioningApi: {
    preflight: vi.fn(),
    plan: vi.fn(),
    start: vi.fn(),
    history: vi.fn(),
    cancel: vi.fn(),
  },
}));
const preflight: Preflight = {
  facts: {
    osId: 'ubuntu',
    versionId: '24.04',
    architecture: 'amd64',
    systemd: true,
    apt: true,
    administrativeAccess: true,
    repositoryReachable: true,
    availableBytes: 4000000000,
    conflictingPackages: false,
    customRepository: false,
    dockerStateKnown: true,
    engineInstalled: false,
    composeInstalled: false,
    additionalPackagesRequired: false,
    serviceRunning: false,
    serviceEnabled: false,
    repositoryValid: false,
  },
  checks: [],
  username: 'tester',
  privilege: 'passwordless',
  codename: 'noble',
  kernel: '6.8',
  dockerSource: 'unknown',
  clientVersion: '',
  serverVersion: '',
  composeVersion: '',
  dockerAccess: 'unavailable',
  dockerGroup: false,
  pendingLogin: false,
  workloads: null,
  checkedAt: '2026-11-04T00:00:00Z',
};
const plan: ProvisioningPlan = {
  id: 'plan',
  serverId: 'server',
  planHash: 'hash',
  expiresAt: '2099-01-01T00:00:00Z',
  preflight,
  steps: [
    { key: 'engine', status: 'REQUIRED', description: 'Install Docker Engine' },
  ],
  grantAccess: false,
  warnings: ['Docker access is root-equivalent.'],
};
const run: ProvisioningRun = {
  approvedPlan: null,
  logs: [],
  id: 'run',
  serverId: 'server',
  planHash: 'hash',
  status: 'RUNNING',
  startedAt: '2026-11-04T00:00:00Z',
  finishedAt: null,
  errorCode: null,
  errorMessage: null,
  cancellationRequested: false,
  sequence: 1,
  steps: [
    {
      key: 'engine',
      description: 'Install Docker Engine',
      status: 'RUNNING',
      startedAt: null,
      finishedAt: null,
      errorCode: null,
      sanitizedOutput: 'Unpacking docker-ce',
    },
  ],
  verification: null,
};
function page() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ProvisioningPanel id="server" lease="lease" visible onBusy={vi.fn()} />
    </QueryClientProvider>,
  );
}
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(provisioningApi.history).mockResolvedValue([]);
  vi.mocked(provisioningApi.plan).mockResolvedValue(plan);
  vi.mocked(provisioningApi.preflight).mockResolvedValue(preflight);
  vi.mocked(provisioningApi.start).mockResolvedValue(run);
});
test('opening the tab only loads local history', async () => {
  page();
  await waitFor(() => expect(provisioningApi.history).toHaveBeenCalled());
  expect(provisioningApi.preflight).not.toHaveBeenCalled();
  expect(provisioningApi.start).not.toHaveBeenCalled();
});
test('installation requires review and explicit confirmation', async () => {
  page();
  fireEvent.click(screen.getByText('Inspect and plan provisioning'));
  fireEvent.click(await screen.findByText('Review confirmation'));
  expect(provisioningApi.start).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText('Approve and execute'));
  await waitFor(() =>
    expect(provisioningApi.start).toHaveBeenCalledWith(
      'server',
      'lease',
      'plan',
      false,
    ),
  );
});
test('blocked plans cannot be confirmed', async () => {
  vi.mocked(provisioningApi.plan).mockResolvedValue({
    ...plan,
    steps: [
      {
        key: 'conflict',
        status: 'BLOCKED',
        description: 'Resolve package conflicts manually.',
      },
    ],
  });
  page();
  fireEvent.click(screen.getByText('Inspect and plan provisioning'));
  expect(await screen.findByText('Review confirmation')).toBeDisabled();
  expect(provisioningApi.start).not.toHaveBeenCalled();
});
test('Docker access has a separate root-equivalent warning and approval', async () => {
  vi.mocked(provisioningApi.plan).mockResolvedValue({
    ...plan,
    grantAccess: true,
  });
  page();
  fireEvent.click(screen.getByText('Grant Docker access to this SSH user'));
  await waitFor(() =>
    expect(provisioningApi.plan).toHaveBeenCalledWith('server', 'lease', true),
  );
  fireEvent.click(await screen.findByText('Review confirmation'));
  expect(
    screen.getByText('Grant root-equivalent Docker access?'),
  ).toBeInTheDocument();
  expect(provisioningApi.start).not.toHaveBeenCalled();
});
test('running operations show actual step counts and cooperative cancellation', async () => {
  vi.mocked(provisioningApi.history).mockResolvedValue([run]);
  page();
  expect(await screen.findByText('0 of 1 steps completed')).toBeInTheDocument();
  expect(screen.getByText('Unpacking docker-ce')).toBeInTheDocument();
  fireEvent.click(screen.getByText('Request cancellation'));
  await waitFor(() =>
    expect(provisioningApi.cancel).toHaveBeenCalledWith('server', 'run'),
  );
});

test('expired plans require regeneration', async () => {
  vi.mocked(provisioningApi.plan).mockResolvedValue({
    ...plan,
    expiresAt: '2000-01-01T00:00:00Z',
  });
  page();
  fireEvent.click(screen.getByText('Inspect and plan provisioning'));
  expect(await screen.findByText('Review confirmation')).toBeDisabled();
  expect(
    screen.getByText('Plan expired. Refresh and approve a new plan.'),
  ).toBeInTheDocument();
});

test('history selection shows persisted approved plan and observed logs', async () => {
  vi.mocked(provisioningApi.history).mockResolvedValue([
    {
      ...run,
      approvedPlan: plan,
      logs: [
        {
          id: 'log',
          stepKey: 'engine',
          observedAt: '2026-11-04T00:00:00Z',
          level: 'INFO',
          message: 'Unpacking docker-ce',
        },
      ],
    },
  ]);
  page();
  fireEvent.click(await screen.findByRole('button', { name: /2026.*RUNNING/ }));
  expect(
    screen.getByRole('region', { name: 'Historical provisioning run' }),
  ).toHaveTextContent('Approved plan plan');
});
