import { describe, expect, it } from 'vitest';
import { historyRate, buildHistoryCsv } from './history';

describe('retained history', () => {
  it('uses measured duration instead of assuming a full bucket', () => {
    expect(historyRate(3000, 1500)).toBe(2000);
    expect(historyRate(3000, 0)).toBeNull();
  });
  it('exports four counters and measured duration with stable UTC timestamps', () => {
    const csv = buildHistoryCsv([{ timestampSecs: 0, durationMillis: 1500, bytes: [1, 2, 3, 4] }], true);
    expect(csv).toContain('1970-01-01T00:00:00.000Z,1500,1,2,3,4');
    expect(buildHistoryCsv([{ timestampSecs: 0, durationMillis: 1, bytes: [1, 2, 0, 0] }], false)).toContain('1,2,N/A,N/A');
  });
});
