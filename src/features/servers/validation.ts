import type { ServerInput } from '@/lib/ipc';
export type FieldErrors = Partial<Record<keyof ServerInput, string>>;
export function validateServer(input: ServerInput): FieldErrors {
  const errors: FieldErrors = {};
  if (
    !input.name.trim() ||
    [...input.name].length > 255 ||
    /[\x00-\x1f\x7f-\x9f]/.test(input.name)
  )
    errors.name =
      'Enter a name of 1–255 characters without control characters.';
  if (!Number.isInteger(input.port) || input.port < 1 || input.port > 65535)
    errors.port = 'Port must be between 1 and 65535.';
  // Rust is authoritative for IP parsing, including IPv6. Reject URL syntax locally.
  const host = input.host;
  const dns =
    host.length > 0 &&
    host.length <= 253 &&
    host
      .split('.')
      .every((p) => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/i.test(p));
  const ipv4 =
    /^\d+\.\d+\.\d+\.\d+$/.test(host) &&
    host
      .split('.')
      .every((p) => Number(p) <= 255 && (p === '0' || !p.startsWith('0')));
  let ipv6 = false;
  if (host.includes(':') && /^[a-f0-9:.]+$/i.test(host)) {
    try {
      ipv6 = new URL(`http://[${host}]/`).hostname.startsWith('[');
    } catch {
      /* Rust also rejects malformed IPv6. */
    }
  }
  if ((!dns && !ipv6) || (/^[\d.]+$/.test(host) && !ipv4))
    errors.host = 'Enter a hostname or IP address, without a URL or port.';
  if (!/^[a-z_][a-z0-9_-]{0,63}$/i.test(input.username))
    errors.username =
      'Use a username beginning with a letter or underscore, up to 64 characters.';
  if (
    input.notes &&
    ([...input.notes].length > 4096 ||
      /[\x00-\x08\x0b-\x1f\x7f-\x9f]/.test(input.notes))
  )
    errors.notes =
      'Notes must contain at most 4096 characters without control characters except tabs and newlines.';
  return errors;
}
