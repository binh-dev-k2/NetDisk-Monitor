import { describe, expect, it } from 'vitest';
import { emptySnapshot, systemSnapshot } from './useMonitorSnapshot';

describe('snapshot stream separation', () => {
  it('keeps process diagnostics out of the system snapshot normalization', () => {
    const normalized = systemSnapshot({ ...emptySnapshot, processSequence: 4, processes: [],
      processNetworkError: 'Process network measurement is unavailable' });
    expect(normalized).toEqual(emptySnapshot);
  });
});
