import { describe, expect, it } from 'vitest';
import { groupProcesses, processNetworkText, processActionTarget, sameProcessMetrics } from './process-model';
import type { ProcessMetric } from '../types';

function process(pid: string, startTime: number, active = true): ProcessMetric {
  return { pid, identity: `${pid}:${startTime}`, startTime, active, name: 'agent64.exe', memoryBytes: 10, diskReadBps: 0, diskWriteBps: 0, sessionDiskRead: 100, sessionDiskWrite: 200, uptimeSecs: 1, networkConnections: 0, networkAvailable: false, networkDownBps: null, networkUpBps: null };
}
describe('process model', () => {
  it('compares process fields without serializing the whole payload', () => {
    const original = process('4321', 10);
    expect(sameProcessMetrics([original], [{ ...original }])).toBe(true);
    expect(sameProcessMetrics([original], [{ ...original, sessionDiskRead: 300 }])).toBe(false);
    expect(sameProcessMetrics([original], [{ ...original, startTime: 20, identity: '4321:20' }])).toBe(false);
    expect(sameProcessMetrics([original], [{ ...original, networkDownBps: 0 }])).toBe(false);
    expect(sameProcessMetrics([original], [])).toBe(false);
  });
  it('uses the longest observed duration for a group, not the sum', () => {
    const group = groupProcesses([{ ...process('1', 10), uptimeSecs: 100 }, { ...process('2', 20), uptimeSecs: 40 }])[0];
    expect(group.uptimeSecs).toBe(100);
  });
  it('targets creation time and identity, never a group ID or digits in a name', () => {
    expect(processActionTarget(process('4321', 10))).toEqual({ pid: '4321', startTime: 10, identity: '4321:10' });
    expect(processActionTarget(process('4321', 10, false))).toBeNull();
    expect(processActionTarget(process('group-agent64.exe', 10))).toBeNull();
  });
  it('preserves actual PID separately from stable row identity', () => {
    const group = groupProcesses([process('4321', 10)])[0];
    expect(group.pid).toBe('4321');
    expect(group.children[0].id).toBe('4321:10');
    expect(group.children[0].pid).toBe('4321');
  });
  it('distinguishes PID reuse and counts only active processes', () => {
    const group = groupProcesses([process('4321', 10, false), process('4321', 20)])[0];
    expect(group.children.map(child => child.id)).toEqual(['4321:10', '4321:20']);
    expect(group.activeCount).toBe(1);
    expect(group.sessionDiskRead).toBe(200);
  });
  it('does not present unavailable network measurement as zero', () => {
    expect(processNetworkText(null, false)).toBe('N/A');
    expect(processNetworkText(0, true)).toBe('0 B/s');
  });
  it.each([null, undefined])('keeps incomplete grouped network totals unknown (%s)', missing => {
    const group = groupProcesses([
      { ...process('1', 10), sessionNetworkDown: 100, sessionNetworkUp: 200 },
      { ...process('2', 20, false), sessionNetworkDown: missing, sessionNetworkUp: 300 },
    ])[0];
    expect(group.sessionNetworkDown).toBeNull();
    expect(group.sessionNetworkUp).toBe(500);
  });
  it('retains measured totals from inactive processes without treating them as live rates', () => {
    const group = groupProcesses([
      { ...process('1', 10), networkAvailable: true, networkDownBps: 10, networkUpBps: 20,
        sessionNetworkDown: 0, sessionNetworkUp: 0 },
      { ...process('2', 20, false), networkDownBps: 999, networkUpBps: 999,
        sessionNetworkDown: 100, sessionNetworkUp: 200 },
    ])[0];
    expect(group.activeCount).toBe(1);
    expect(group.sessionNetworkDown).toBe(100);
    expect(group.sessionNetworkUp).toBe(200);
    expect(group.networkDownBps).toBe(10);
    expect(group.networkUpBps).toBe(20);
    const retired = groupProcesses([{ ...process('2', 20, false), sessionNetworkDown: 100, sessionNetworkUp: 200 }])[0];
    expect(retired.networkAvailable).toBe(false);
    expect(retired.networkDownBps).toBeNull();
    expect(retired.sessionNetworkDown).toBe(100);
  });
  it('does not fabricate a zero rate for a missing direction in an available group', () => {
    const group = groupProcesses([
      { ...process('1', 10), networkAvailable: true, networkDownBps: null, networkUpBps: 20 },
      { ...process('2', 20), networkAvailable: true, networkDownBps: 10, networkUpBps: 30 },
    ])[0];
    expect(group.networkDownBps).toBeNull();
    expect(group.networkUpBps).toBe(50);
  });
  it.each(['sessionNetworkDown', 'sessionNetworkUp'] as const)('detects realtime changes to %s', key => {
    const original = { ...process('1', 10), sessionNetworkDown: 100, sessionNetworkUp: 200 };
    expect(sameProcessMetrics([original], [{ ...original, [key]: 300 }])).toBe(false);
    expect(sameProcessMetrics([original], [{ ...original, [key]: null }])).toBe(false);
    expect(sameProcessMetrics([original], [{ ...original, [key]: undefined }])).toBe(false);
    expect(sameProcessMetrics([original], [{ ...original }])).toBe(true);
  });
  it('passes an opaque backend identity unchanged to a close action', () => {
    const record = { ...process('4321', 10), identity: '4321:10:observed-generation-7' };
    expect(processActionTarget(record)).toEqual({ pid: '4321', startTime: 10, identity: record.identity });
  });
});
