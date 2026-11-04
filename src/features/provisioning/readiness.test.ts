import { expect, test } from 'vitest';
import { readiness } from './readiness';

test('cached or absent observations never establish readiness', () => {
  expect(readiness(undefined, undefined, true, false)).toBe(
    'Unknown · run checks',
  );
});
test('active checking takes priority over cached observations', () => {
  expect(readiness(undefined, undefined, true, true)).toBe('Checking');
});
