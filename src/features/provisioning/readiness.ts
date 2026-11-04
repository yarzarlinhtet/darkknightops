import type { Preflight, ProvisioningRun } from '@/lib/bindings';

export function readiness(
  value: Preflight | undefined,
  run: ProvisioningRun | undefined,
  stale: boolean,
  checking: boolean,
): string {
  if (checking) return 'Checking';
  if (run && ['READY', 'RUNNING'].includes(run.status)) return 'Provisioning';
  if (!value || stale) return 'Unknown · run checks';
  const facts = value.facts;
  if (
    facts.osId !== 'ubuntu' ||
    facts.versionId !== '24.04' ||
    !['amd64', 'arm64'].includes(facts.architecture)
  )
    return 'Unsupported';
  if (value.checks.some((check) => check.status === 'BLOCKED')) return 'Failed';
  if (
    facts.engineInstalled &&
    facts.composeInstalled &&
    facts.serviceRunning &&
    facts.serviceEnabled &&
    ['direct', 'administrative'].includes(value.dockerAccess)
  )
    return 'Ready';
  if (facts.engineInstalled || facts.composeInstalled)
    return 'Partially Configured';
  return 'Setup Required';
}
