export type ProcessMetric = {
  pid: string;
  identity: string;
  startTime: number;
  active: boolean;
  name: string;
  memoryBytes: number;
  diskReadBps: number;
  diskWriteBps: number;
  sessionDiskRead: number;
  sessionDiskWrite: number;
  uptimeSecs: number;
  networkConnections: number;
  networkAvailable: boolean;
  networkDownBps: number | null;
  networkUpBps: number | null;
  sessionNetworkDown?: number | null;
  sessionNetworkUp?: number | null;
};

export type SystemSnapshot = {
  sequence: number;
  storageError?: string | null;
  taskbarError?: string | null;
  diskAvailable: boolean;
  networkDownBps: number;
  networkUpBps: number;
  diskReadBps: number;
  diskWriteBps: number;
  sessionNetworkDown: number;
  sessionNetworkUp: number;
  sessionDiskRead: number;
  sessionDiskWrite: number;
  sessionDurationSecs: number;
  interfaces: string[];
  totalMemoryBytes: number;
  usedMemoryBytes: number;
};

export type ProcessSnapshot = { processSequence: number; processes: ProcessMetric[]; processNetworkError?: string | null };
export type Snapshot = SystemSnapshot & ProcessSnapshot;
export type History = Record<'down' | 'up' | 'read' | 'write', number[]>;

export type ProcessGroup = Omit<ProcessMetric, 'identity'> & {
  id: string;
  activeCount: number;
  isGroup: boolean;
  children: Array<ProcessMetric & { id: string }>;
};
