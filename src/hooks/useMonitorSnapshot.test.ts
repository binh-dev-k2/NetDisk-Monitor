import { describe, expect, it } from 'vitest';
import { appendHistorySample, emptySnapshot, systemSnapshot } from './useMonitorSnapshot';

describe('snapshot stream separation', () => {
  it('initializes the cached active process count without process rows', () => {
    expect(emptySnapshot).toHaveProperty('activeProcessCount', null);
  });

  it('preserves cached summary fields while stripping legacy process data', () => {
    const summary = { ...emptySnapshot, activeProcessCount: 73, processNetworkStatusError: 'Summary unavailable' };
    expect(systemSnapshot({ ...summary, processSequence: 4, processes: [], processNetworkError: 'Legacy error' })).toEqual(summary);
  });

  it('preserves explicit null and absent summary diagnostics', () => {
    expect(systemSnapshot({ ...emptySnapshot, activeProcessCount: 0, processNetworkStatusError: null }))
      .toEqual({ ...emptySnapshot, activeProcessCount: 0, processNetworkStatusError: null });
    expect(systemSnapshot(emptySnapshot)).not.toHaveProperty('processNetworkStatusError');
  });

  it('distinguishes an unavailable summary from an observed empty process list', () => {
    const unavailable = { ...emptySnapshot, activeProcessCount: null, processNetworkStatusError: 'Summary lock unavailable' };
    expect(systemSnapshot(unavailable)).toEqual(unavailable);
    expect(systemSnapshot(unavailable).activeProcessCount).toBeNull();
    expect(systemSnapshot({ ...emptySnapshot, activeProcessCount: 0 }).activeProcessCount).toBe(0);
  });

  it('keeps process diagnostics out of the system snapshot normalization', () => {
    const normalized = systemSnapshot({ ...emptySnapshot, processSequence: 4, processes: [],
      processNetworkError: 'Process network measurement is unavailable' });
    expect(normalized).toEqual(emptySnapshot);
  });

  it('bounds the rolling sparkline history ring to 30 samples', () => {
    let ring = { down: [] as number[], up: [] as number[], read: [] as number[], write: [] as number[] };
    for (let i = 1; i <= 35; i++) {
      ring = appendHistorySample(ring, {
        ...emptySnapshot, sequence: i, networkDownBps: i, networkUpBps: i * 2, diskReadBps: i * 3, diskWriteBps: i * 4,
      });
    }
    expect(ring.down).toHaveLength(30);
    expect(ring.down[0]).toBe(6);
    expect(ring.down[29]).toBe(35);
    expect(ring.write[29]).toBe(140);
  });
});
