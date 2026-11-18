import { describe, expect, it } from 'vitest';
import { formatBytes, formatDuration, formatRate } from './format';

describe('metric formatters', () => {
  it('formats zero, sub-byte, and non-finite inputs safely', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(0.2)).toBe('0.2 B');
    expect(formatRate(0.2)).toBe('0.2 B/s');
    expect(formatBytes(-5)).toBe('0 B');
    expect(formatBytes(Number.NaN)).toBe('0 B');
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe('0 B');
  });

  it('formats a fractional kilobyte rate', () => {
    expect(formatRate(1536)).toBe('1.5 KB/s');
  });

  it('formats megabytes', () => {
    expect(formatBytes(1_048_576)).toBe('1.0 MB');
  });

  it('formats process uptime as hours, minutes and seconds', () => {
    expect(formatDuration(3_661)).toBe('1h 01m 01s');
  });
});
