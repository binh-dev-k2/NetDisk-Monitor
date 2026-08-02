import { describe, expect, it } from 'vitest';
import { sortProcessGroups } from './process-sort';
import type { ProcessGroup } from '../types';

function group(name: string, memoryBytes: number, diskReadBps: number): ProcessGroup {
  return {
    id: `group-${name}`,
    pid: `PID 1`,
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
    sessionNetDown: 0,
    sessionNetUp: 0,
    isGroup: true,
    children: [],
  };
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
});
