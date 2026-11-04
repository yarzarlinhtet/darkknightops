import { SshKeysPage } from '@/features/keys/keys-page';
import { useEffect } from 'react';
import { HashRouter, NavLink, Route, Routes, Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import {
  LayoutDashboard,
  Server,
  Folder,
  Container,
  Rocket,
  Globe,
  Settings,
  PanelLeftClose,
  PanelLeftOpen,
  Anchor,
  ArrowUpRight,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { usePreferences, type Theme } from '@/store/preferences';
import { ServerDetailPage } from '@/features/servers/server-detail';
import { ServersPage } from '@/features/servers/servers-page';
import { serversQuery } from '@/features/servers/queries';
import { cn } from '@/lib/utils';
const navigation = [
  { label: 'Dashboard', path: '/', icon: LayoutDashboard },
  { label: 'SSH Keys', path: '/ssh-keys', icon: Server },
  { label: 'Servers', path: '/servers', icon: Server },
  { label: 'Projects', path: '/projects', icon: Folder },
  { label: 'Containers', path: '/containers', icon: Container },
  { label: 'Deployments', path: '/deployments', icon: Rocket },
  { label: 'Domains & SSL', path: '/domains', icon: Globe },
  { label: 'Settings', path: '/settings', icon: Settings },
];
function Dashboard() {
  const query = useQuery(serversQuery);
  return (
    <>
      <div className="page-heading">
        <div>
          <p className="eyebrow">Overview</p>
          <h1>
            Your infrastructure.
            <br />
            Your control.
          </h1>
          <p className="mt-3 text-muted-foreground">
            A local workspace for your remote servers.
          </p>
        </div>
        <span className="status-badge">Milestone 2 · SSH discovery</span>
      </div>
      <div className="grid gap-5 lg:grid-cols-2">
        <section className="panel">
          <Server className="mb-6 size-6 text-primary" />
          <h2>Saved servers</h2>
          {query.isPending ? (
            <p role="status" className="mt-3">
              Loading…
            </p>
          ) : query.error ? (
            <p role="alert" className="mt-3 text-sm text-destructive">
              {query.error.message}
            </p>
          ) : (
            <p className="my-3 text-5xl font-semibold">{query.data.length}</p>
          )}
          <Button asChild variant="outline" className="mt-4">
            <Link to="/servers">
              Manage inventory
              <ArrowUpRight />
            </Link>
          </Button>
        </section>
        <section className="panel">
          <p className="eyebrow">Built for your desktop</p>
          <h2 className="mt-3">Start with a server.</h2>
          <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
            Connect through SSH to discover server resources and Docker
            capabilities. Workload management and routing arrive in later
            milestones.
          </p>
          <div className="notice mt-6 mb-0">
            No cloud account or separate backend required.
          </div>
        </section>
      </div>
    </>
  );
}
function SettingsPage() {
  const { theme, setTheme } = usePreferences();
  return (
    <>
      <div className="page-heading">
        <div>
          <p className="eyebrow">Workspace</p>
          <h1>Settings</h1>
        </div>
      </div>
      <section className="panel max-w-2xl">
        <h2>Appearance</h2>
        <p className="mt-2 text-sm text-muted-foreground">
          Use your system appearance or choose a theme.
        </p>
        <label htmlFor="theme" className="mt-6 mb-2 block text-sm font-medium">
          Theme
        </label>
        <select
          id="theme"
          value={theme}
          onChange={(e) => setTheme(e.target.value as Theme)}
          className="w-full rounded-md border bg-background p-2.5"
        >
          {(['system', 'light', 'dark'] as const).map((t) => (
            <option key={t} value={t}>
              {t.charAt(0).toUpperCase() + t.slice(1)}
            </option>
          ))}
        </select>
      </section>
    </>
  );
}
function Unavailable({ title }: { title: string }) {
  return (
    <>
      <div className="page-heading">
        <div>
          <p className="eyebrow">Coming later</p>
          <h1>{title}</h1>
        </div>
      </div>
      <section className="panel empty-state">
        <h2>Not available in Milestone 1</h2>
        <p>
          This milestone provides the desktop foundation and local server
          inventory.
        </p>
        <Button asChild variant="outline">
          <Link to="/servers">Go to servers</Link>
        </Button>
      </section>
    </>
  );
}
export function App() {
  const {
    theme,
    sidebarCollapsed: collapsed,
    toggleSidebar,
  } = usePreferences();
  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = () => {
      document.documentElement.classList.toggle(
        'dark',
        theme === 'dark' || (theme === 'system' && media.matches),
      );
    };
    apply();
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  }, [theme]);
  return (
    <HashRouter>
      <a
        href="#main-content"
        className="skip-link"
        onClick={(event) => {
          event.preventDefault();
          document.getElementById('main-content')?.focus();
        }}
      >
        Skip to content
      </a>
      <div className="flex min-h-screen">
        <aside className={cn('sidebar', collapsed && 'collapsed')}>
          <Link to="/" className="brand" aria-label="RemoteDock home">
            <Anchor className="size-7 shrink-0 text-primary" />
            <span className="nav-label">RemoteDock</span>
          </Link>
          <p className="nav-label px-3 pb-3 text-xs font-medium uppercase tracking-widest text-muted-foreground">
            Workspace
          </p>
          <nav aria-label="Main navigation" className="space-y-1">
            {navigation.map(({ label, path, icon: Icon }) => (
              <NavLink
                key={path}
                to={path}
                end={path === '/'}
                title={label}
                aria-label={label}
                className={({ isActive }) =>
                  cn('nav-item', isActive && 'active')
                }
              >
                <Icon className="size-4 shrink-0" />
                <span className="nav-label">{label}</span>
                <span className="sr-only">{collapsed ? label : ''}</span>
              </NavLink>
            ))}
          </nav>
          <div className="mt-auto pt-10">
            <Button
              variant="ghost"
              className="hidden w-full justify-start md:inline-flex"
              onClick={toggleSidebar}
              aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            >
              {collapsed ? <PanelLeftOpen /> : <PanelLeftClose />}
              <span className="nav-label">Collapse sidebar</span>
            </Button>
            <p className="nav-label mt-4 px-3 text-xs text-muted-foreground">
              Local workspace · v0.1.0
            </p>
          </div>
        </aside>
        <div className="min-w-0 flex-1">
          <header className="flex h-16 items-center justify-between border-b px-6 text-xs text-muted-foreground">
            <span>RemoteDock / Desktop</span>
            <span>Local only</span>
          </header>
          <main
            id="main-content"
            tabIndex={-1}
            className="mx-auto max-w-7xl p-5 outline-none md:p-10"
          >
            <Routes>
              <Route path="/" element={<Dashboard />} />
              <Route path="/ssh-keys" element={<SshKeysPage />} />
              <Route path="/servers" element={<ServersPage />} />
              <Route path="/servers/:id" element={<ServerDetailPage />} />
              <Route path="/settings" element={<SettingsPage />} />
              {navigation.slice(2, 6).map((item) => (
                <Route
                  key={item.path}
                  path={item.path}
                  element={<Unavailable title={item.label} />}
                />
              ))}
              <Route
                path="*"
                element={
                  <>
                    <h1>Page not found</h1>
                    <Link to="/">Return to dashboard</Link>
                  </>
                }
              />
            </Routes>
          </main>
        </div>
      </div>
    </HashRouter>
  );
}
