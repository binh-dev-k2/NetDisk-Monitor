import { describe, expect, it } from 'vitest';
import { buildProcessCsv } from './csv';
import type { ProcessGroup } from '../types';
import { groupProcesses } from './process-model';

function group(name: string, pid: string): ProcessGroup {
  return groupProcesses([{
    pid, identity: `${pid}:10`, startTime: 10, active: true, networkAvailable: false,
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
  }])[0];
}

describe('buildProcessCsv', () => {
  it.each(['=SUM(1,2)', '+cmd', '-cmd', '@cmd', '\tcmd', '\rcmd', '  =SUM(1,2)'])('neutralizes Excel formula-like process name %j', (name: string) => {
    const csv = buildProcessCsv([group(name, '123')]);
    expect(csv).toContain(`"'${name}"`);
  });
  it('writes a BOM header and quoted rows', () => {
    const csv = buildProcessCsv([group('Chrome', '123')]);

    expect(csv.startsWith('\uFEFF')).toBe(true);
    expect(csv).toContain('"Chrome"');
    expect(csv).toContain('"123"');
    expect(csv).toContain('I/O phiên (Bytes)');
    expect(csv).toContain('N/A,N/A');
    expect(csv).toContain('3000');
  });

  it('escapes double quotes in process names', () => {
    const escaped = buildProcessCsv([group('Bad"Name', '1')]);

    expect(escaped).toContain('"Bad""Name"');
  });
  it('exports measured network totals including retained inactive children', () => {
    const record = group('agent.exe', '1').children[0];
    const groups = groupProcesses([
      { ...record, sessionNetworkDown: 100, sessionNetworkUp: 200 },
      { ...record, pid: '2', identity: '2:20', startTime: 20, active: false,
        sessionNetworkDown: 300, sessionNetworkUp: 400 },
    ]);
    const csv = buildProcessCsv(groups);
    expect(csv).toContain('Mạng phiên đã đo (Bytes)');
    expect(csv.split('\r\n')[1].split(',').at(-1)).toBe('1000');
    expect(csv).toContain('"1 | 2"');
  });
  it.each([null, undefined])('exports N/A, never a partial sum, for missing network totals (%s)', missing => {
    const record = group('agent.exe', '1').children[0];
    const groups = groupProcesses([
      { ...record, sessionNetworkDown: 100, sessionNetworkUp: 200 },
      { ...record, identity: '1:20', startTime: 20, active: false,
        sessionNetworkDown: missing, sessionNetworkUp: 0 },
    ]);
    expect(buildProcessCsv(groups).split('\r\n')[1].split(',').at(-1)).toBe('N/A');
  });
  it('distinguishes a measured zero total from unavailable totals', () => {
    const record = group('agent.exe', '1').children[0];
    expect(buildProcessCsv(groupProcesses([{ ...record, sessionNetworkDown: 0, sessionNetworkUp: 0 }]))
      .split('\r\n')[1].split(',').at(-1)).toBe('0');
  });
});
