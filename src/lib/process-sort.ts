import type { ProcessGroup } from '../types';

export type SortColumn =
  | 'name'
  | 'processCount'
  | 'memory'
  | 'uptime'
  | 'netDown'
  | 'netUp'
  | 'diskRead'
  | 'diskWrite'
  | 'totalIO'
  | 'totalNet';

const accessors: Record<SortColumn, (group: ProcessGroup) => string | number> = {
  name: group => group.name.toLowerCase(),
  processCount: group => group.activeCount,
  memory: group => group.memoryBytes,
  uptime: group => group.uptimeSecs,
  netDown: group => group.networkAvailable ? group.networkDownBps ?? -1 : -1,
  netUp: group => group.networkAvailable ? group.networkUpBps ?? -1 : -1,
  diskRead: group => group.diskReadBps,
  diskWrite: group => group.diskWriteBps,
  totalIO: group => group.sessionDiskRead + group.sessionDiskWrite,
  totalNet: group => group.sessionNetworkDown != null && group.sessionNetworkUp != null ? group.sessionNetworkDown + group.sessionNetworkUp : -1,
};

export function sortProcessGroups(
  groups: ProcessGroup[],
  column: SortColumn,
  direction: 'asc' | 'desc',
): ProcessGroup[] {
  const accessor = accessors[column] ?? accessors.name;
  const sign = direction === 'asc' ? 1 : -1;
  return [...groups].sort((left, right) => {
    const leftValue = accessor(left);
    const rightValue = accessor(right);
    if (leftValue < rightValue) return -sign;
    if (leftValue > rightValue) return sign;
    return left.id.localeCompare(right.id);
  });
}
