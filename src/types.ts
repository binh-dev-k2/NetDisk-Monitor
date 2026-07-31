export type ProcessMetric = {
  pid: string;
  name: string;
  memoryBytes: number;
  diskReadBps: number;
  diskWriteBps: number;
  sessionDiskRead: number;
  sessionDiskWrite: number;
  uptimeSecs: number;
  networkConnections: number;
  networkDownBps: number;
  networkUpBps: number;
};

export type Snapshot = {
  networkDownBps: number;
  networkUpBps: number;
  diskReadBps: number;
  diskWriteBps: number;
  sessionNetworkDown: number;
  sessionNetworkUp: number;
  sessionDiskRead: number;
  sessionDiskWrite: number;
  sessionDurationSecs: number;
  processes: ProcessMetric[];
  interfaces: string[];
  totalMemoryBytes: number;
};

export type History = Record<'down' | 'up' | 'read' | 'write', number[]>;

export type ProcessGroup = {
  id: string;
  pid: string;
  name: string;
  memoryBytes: number;
  diskReadBps: number;
  diskWriteBps: number;
  sessionDiskRead: number;
  sessionDiskWrite: number;
  uptimeSecs: number;
  networkConnections: number;
  networkDownBps: number;
  networkUpBps: number;
  sessionNetDown: number;
  sessionNetUp: number;
  isGroup: boolean;
  children: Array<{
    id: string;
    pid: string;
    name: string;
    memoryBytes: number;
    diskReadBps: number;
    diskWriteBps: number;
    sessionDiskRead: number;
    sessionDiskWrite: number;
    uptimeSecs: number;
    networkConnections: number;
    networkDownBps: number;
    networkUpBps: number;
    sessionNetDown: number;
    sessionNetUp: number;
  }>;
};
