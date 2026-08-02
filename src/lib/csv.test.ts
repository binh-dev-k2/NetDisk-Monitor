import { describe, expect, it } from 'vitest';
import { buildProcessCsv } from './csv';
import type { ProcessGroup } from '../types';

function group(name: string, pid: string): ProcessGroup {
  return {
    id: `group-${name}`,
    pid: `PID ${pid}`,
    name,
    memoryBytes: 100,
    diskReadBps: 10,
    diskWriteBps: 20,
    sessionDiskRead: 1_000,
    sessionDiskWrite: 2_000,
    uptimeSecs: 60,
    networkConnections: 2,
    networkDownBps: 30,
    networkUpBps: 40,
    sessionNetDown: 3_000,
    sessionNetUp: 4_000,
    isGroup: true,
    children: [],
  };
}

describe('buildProcessCsv', () => {
  it('writes a BOM header and quoted rows', () => {
    const csv = buildProcessCsv([group('Chrome', '123')]);

    expect(csv.startsWith('\uFEFF')).toBe(true);
    expect(csv).toContain('"Chrome"');
    expect(csv).toContain('"PID 123"');
    expect(csv).toContain('Tổng I/O Đĩa (Bytes)');
    expect(csv).toContain('3000');
  });

  it('escapes double quotes in process names', () => {
    const escaped = buildProcessCsv([group('Bad"Name', '1')]);

    expect(escaped).toContain('"Bad""Name"');
  });
});
