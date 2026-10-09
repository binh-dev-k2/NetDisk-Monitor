import { describe, expect, it } from 'vitest';
import { sortProcessGroups } from './process-sort';
import type { ProcessGroup } from '../types';
import { groupProcesses } from './process-model';

function group(name: string, memoryBytes: number, diskReadBps: number): ProcessGroup {
  return groupProcesses([{
    pid: '1', identity: '1:10', startTime: 10, active: true, networkAvailable: false,
    name,
    memoryBytes,
    diskReadBps,
    diskWriteBps: 0,
    sessionDiskRead: 0,
    sessionDiskWrite: 0,
    uptimeSecs: 0,
    networkConnections: 0,
    networkDownBps: 0,
    networkUpBps: 0,
  }])[0];
}

describe('sortProcessGroups', () => {
  it('sorts by name ascending case-insensitively', () => {
    const sorted = sortProcessGroups([group('zeta', 0, 0), group('Alpha', 0, 0)], 'name', 'asc');

    expect(sorted.map(g => g.name)).toEqual(['Alpha', 'zeta']);
  });

  it('sorts by memory descending', () => {
    const sorted = sortProcessGroups([group('a', 100, 0), group('b', 900, 0)], 'memory', 'desc');

    expect(sorted.map(g => g.name)).toEqual(['b', 'a']);
  });

  it('sorts by disk read rate ascending', () => {
    const sorted = sortProcessGroups([group('a', 0, 50), group('b', 0, 10)], 'diskRead', 'asc');

    expect(sorted.map(g => g.name)).toEqual(['b', 'a']);
  });

  it('does not mutate the input array', () => {
    const input = [group('a', 0, 0), group('b', 0, 0)];
    const snapshot = [...input];

    sortProcessGroups(input, 'name', 'desc');

    expect(input).toEqual(snapshot);
  });
  it('sorts measured session-network totals separately from unknown totals and current rates', () => {
    const groups = [
      { ...group('unknown', 0, 0), sessionNetworkDown: null, sessionNetworkUp: 10000 },
      { ...group('zero', 0, 0), sessionNetworkDown: 0, sessionNetworkUp: 0 },
      { ...group('retired', 0, 0), active: false, sessionNetworkDown: 100, sessionNetworkUp: 200 },
    ];
    expect(sortProcessGroups(groups, 'totalNet', 'asc').map(item => item.name)).toEqual(['unknown', 'zero', 'retired']);
    expect(sortProcessGroups(groups, 'totalNet', 'desc').map(item => item.name)).toEqual(['retired', 'zero', 'unknown']);
  });
});
