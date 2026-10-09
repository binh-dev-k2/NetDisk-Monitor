import type { ProcessGroup, ProcessMetric } from '../types';
import { formatRate } from './format';

const processFields: readonly (keyof ProcessMetric)[] = [
  'pid', 'identity', 'startTime', 'active', 'name', 'memoryBytes', 'diskReadBps', 'diskWriteBps',
  'sessionDiskRead', 'sessionDiskWrite', 'uptimeSecs', 'networkConnections', 'networkAvailable',
  'networkDownBps', 'networkUpBps', 'sessionNetworkDown', 'sessionNetworkUp',
];

export function sameProcessMetrics(left: readonly ProcessMetric[], right: readonly ProcessMetric[]): boolean {
  return left.length === right.length && left.every((process, index) =>
    processFields.every(field => process[field] === right[index][field]));
}

export function processNetworkText(rate: number | null, available: boolean): string {
  return available && rate !== null ? formatRate(rate) : 'N/A';
}

export function processActionTarget(process: ProcessMetric): { pid: string; startTime: number; identity: string } | null {
  if (!process.active || !/^\d+$/.test(process.pid) || !Number.isFinite(process.startTime) || process.startTime < 0) return null;
  return { pid: process.pid, startTime: process.startTime, identity: process.identity };
}

export function groupProcesses(processes: ProcessMetric[]): ProcessGroup[] {
  const groups = new Map<string, ProcessMetric[]>();
  for (const process of processes) {
    const list = groups.get(process.name) ?? [];
    list.push(process);
    groups.set(process.name, list);
  }
  return Array.from(groups, ([name, list]) => {
    const children = list.map(process => ({ ...process, id: process.identity }));
    const active = list.filter(process => process.active);
    const sum = (key: 'memoryBytes' | 'diskReadBps' | 'diskWriteBps' | 'sessionDiskRead' | 'sessionDiskWrite' | 'networkConnections') => list.reduce((total, process) => total + process[key], 0);
    const networkAvailable = active.length > 0 && active.every(process => process.networkAvailable);
    const totalNetwork = (key: 'sessionNetworkDown' | 'sessionNetworkUp') =>
      list.every(process => process[key] != null) ? list.reduce((total, process) => total + (process[key] ?? 0), 0) : null;
    const networkRate = (key: 'networkDownBps' | 'networkUpBps') =>
      networkAvailable && active.every(process => process[key] != null)
        ? active.reduce((total, process) => total + (process[key] ?? 0), 0) : null;
    return {
      id: `group-${name}`, name, pid: list[0].pid, startTime: list[0].startTime,
      active: active.length > 0, activeCount: active.length, isGroup: list.length > 1, children,
      memoryBytes: sum('memoryBytes'), diskReadBps: sum('diskReadBps'), diskWriteBps: sum('diskWriteBps'),
      sessionDiskRead: sum('sessionDiskRead'), sessionDiskWrite: sum('sessionDiskWrite'),
      sessionNetworkDown: totalNetwork('sessionNetworkDown'), sessionNetworkUp: totalNetwork('sessionNetworkUp'),
      uptimeSecs: Math.max(...list.map(process => process.uptimeSecs)), networkConnections: sum('networkConnections'), networkAvailable,
      networkDownBps: networkRate('networkDownBps'),
      networkUpBps: networkRate('networkUpBps'),
    };
  });
}
