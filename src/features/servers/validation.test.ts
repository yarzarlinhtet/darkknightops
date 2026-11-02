import { expect, test } from 'vitest';
import { validateServer } from './validation';
const base = {
  name: 'Production',
  host: 'vps.example.com',
  username: 'ubuntu',
  port: 22,
  notes: null,
};
test('accepts hostname, IP and field boundaries', () => {
  for (const host of [
    'example.com',
    '192.0.2.1',
    '2001:db8::1',
    '::ffff:192.0.2.1',
  ])
    expect(validateServer({ ...base, host })).toEqual({});
  expect(
    validateServer({
      ...base,
      port: 65535,
      name: 'a'.repeat(255),
      notes: 'n'.repeat(4096),
    }),
  ).toEqual({});
});
test('rejects invalid metadata before IPC', () => {
  for (const host of [
    'https://example.com',
    'example.com:22',
    'a;id',
    '-host.com',
    '999.0.0.1',
    ':::',
    '',
  ])
    expect(validateServer({ ...base, host })).toHaveProperty('host');
  for (const port of [0, 65536, 22.5, NaN])
    expect(validateServer({ ...base, port })).toHaveProperty('port');
  expect(
    validateServer({ ...base, name: ' ', username: '-root', notes: '\0' }),
  ).toMatchObject({
    name: expect.any(String),
    username: expect.any(String),
    notes: expect.any(String),
  });
});
