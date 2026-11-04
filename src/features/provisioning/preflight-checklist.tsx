import { CheckCircle2, AlertTriangle, CircleHelp } from 'lucide-react';
import type { Preflight } from '@/lib/bindings';

export function PreflightChecklist({ value }: { value: Preflight }) {
  const facts = value.facts;
  const groups = [
    {
      name: 'System Compatibility',
      checks: [
        ['SSH connectivity', true, `Authenticated as ${value.username}`],
        [
          'Operating system',
          facts.osId === 'ubuntu' &&
            facts.versionId === '24.04' &&
            ['amd64', 'arm64'].includes(facts.architecture),
          'Ubuntu 24.04 on amd64 or arm64 required',
        ],
        [
          'Package manager',
          facts.apt && facts.systemd,
          'apt and running systemd required',
        ],
        [
          'Disk space',
          facts.availableBytes >= 2 * 1024 ** 3,
          `${(facts.availableBytes / 1024 ** 3).toFixed(1)} GiB available; 2 GiB required for installation`,
        ],
      ],
    },
    {
      name: 'Administrative Access',
      checks: [
        [
          'Administrative privileges',
          facts.administrativeAccess,
          `Access: ${value.privilege}. Root or appropriate passwordless sudo required.`,
        ],
      ],
    },
    {
      name: 'Network and Package Repositories',
      checks: [
        [
          'Repository connectivity',
          facts.repositoryReachable,
          'Check DNS and HTTPS connectivity to Ubuntu and Docker repositories.',
        ],
      ],
    },
    {
      name: 'Docker Installation',
      checks: [
        [
          'Docker Engine',
          facts.dockerStateKnown ? facts.engineInstalled : null,
          value.serverVersion ||
            value.clientVersion ||
            'Install through a reviewed provisioning plan.',
        ],
        [
          'Docker Compose',
          facts.dockerStateKnown ? facts.composeInstalled : null,
          value.composeVersion ||
            'Install the Compose plugin through a reviewed plan.',
        ],
        [
          'Docker daemon',
          facts.dockerStateKnown ? facts.serviceRunning : null,
          facts.serviceEnabled
            ? 'Enabled at startup'
            : 'Not enabled at startup',
        ],
      ],
    },
    {
      name: 'Docker Permissions',
      checks: [
        [
          'Docker access',
          value.dockerAccess === 'direct',
          `${value.username} · ${value.dockerAccess}; Docker group: ${value.dockerGroup ? 'member' : 'not member'}. Group access grants root-equivalent privileges.`,
        ],
      ],
    },
  ];
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      {groups.map((group) => (
        <section className="rounded-lg border p-4" key={group.name}>
          <h3 className="mb-3 text-sm font-semibold">{group.name}</h3>
          <ul className="space-y-3">
            {group.checks.map(([name, passed, detail]) => {
              const Icon =
                passed == null
                  ? CircleHelp
                  : passed
                    ? CheckCircle2
                    : AlertTriangle;
              return (
                <li key={String(name)}>
                  <details>
                    <summary className="flex cursor-pointer items-center gap-2 text-sm">
                      <Icon
                        aria-hidden
                        className={`size-4 shrink-0 ${passed ? 'text-primary' : 'text-muted-foreground'}`}
                      />
                      <span>{name}</span>
                      <span className="ml-auto text-xs">
                        {passed == null
                          ? 'Unknown'
                          : passed
                            ? 'Passed'
                            : 'Needs attention'}
                      </span>
                    </summary>
                    <p className="mt-2 text-xs text-muted-foreground">
                      {detail}
                    </p>
                  </details>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}
