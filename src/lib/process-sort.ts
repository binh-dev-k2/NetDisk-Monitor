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
  processCount: group => group.children.length,
  memory: group => group.memoryBytes,
  uptime: group => group.uptimeSecs,
  netDown: group => group.networkDownBps,
  netUp: group => group.networkUpBps,
  diskRead: group => group.diskReadBps,
  diskWrite: group => group.diskWriteBps,
  totalIO: group => group.sessionDiskRead + group.sessionDiskWrite,
  totalNet: group => group.sessionNetDown + group.sessionNetUp,
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
    return 0;
  });
}
