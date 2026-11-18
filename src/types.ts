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

export type TaskbarPhase = 'disabled' | 'starting' | 'attached' | 'degraded' | 'unsupported';

export type TaskbarStatus = {
  phase: TaskbarPhase;
  desired: boolean;
  attached: boolean;
  reason?: string | null;
};

export type ThemeMode = 'system' | 'dark' | 'light';

export type WorkerHealthItem = {
  kind: 'system' | 'processes' | 'storage';
  status: 'notStarted' | 'running' | 'warning' | 'failed' | 'stopped';
  heartbeatAgeMillis: number | null;
  ticks: number;
  lastError: string | null;
};

export type RuntimeHealth = {
  processId: number;
  ignoredExitRequests: number;
  workers: WorkerHealthItem[];
  logging: {
    droppedEvents: number;
    lastError: string | null;
    active: boolean;
  } | null;
};

export type AppTab = 'overview' | 'processes' | 'disk' | 'history' | 'settings';

export type VolumeUsage = {
  mountPoint: string;
  name: string;
  fileSystem: string;
  totalBytes: number;
  availableBytes: number;
  usedBytes: number;
  isRemovable: boolean;
};

export type SystemSnapshot = {
  sequence: number;
  activeProcessCount: number | null;
  processNetworkStatusError?: string | null;
  storageError?: string | null;
  taskbarError?: string | null;
  taskbarStatus?: TaskbarStatus;
  diskAvailable: boolean;
  cpuUsagePercent?: number;
  volumes?: VolumeUsage[];
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

export type ClearTarget = 'diagnostics' | 'caches' | 'history' | 'all_clearable';

export type StorageItem = {
  id: string;
  label: string;
  path: string;
  bytes: number;
  maxBytes: number | null;
  recordCount: number | null;
  clearable: boolean;
  detail: string;
};

export type MemoryCacheStats = {
  iconCacheCount: number;
  iconCacheLimit: number;
  terminatedProcessCount: number;
  terminatedProcessLimit: number;
};

export type StorageOverview = {
  appDataDir: string;
  webViewDataDir: string | null;
  totalAppManagedBytes: number;
  maxAppManagedBytes: number;
  totalWebViewBytes: number;
  items: StorageItem[];
  memory: MemoryCacheStats;
};

export type CleanupRuleId =
  | 'temp_files'
  | 'crash_dumps'
  | 'wer_reports'
  | 'directx_shader_cache'
  | 'npm_cache'
  | 'pnpm_cache'
  | 'pip_cache'
  | 'cargo_registry_cache'
  | 'nuget_http_cache';

export type CleanupRuleFinding = {
  id: CleanupRuleId;
  category: string;
  label: string;
  description: string;
  path: string;
  bytes: number;
  fileCount: number;
  exists: boolean;
  safetyNote: string;
};

export type CleanupScanReport = {
  rules: CleanupRuleFinding[];
  totalReclaimableBytes: number;
  totalFiles: number;
};

export type CleanupExecutionReport = {
  freedBytes: number;
  deletedFiles: number;
  skippedLockedFiles: number;
  refreshed: CleanupScanReport;
};

export type SpaceNode = {
  name: string;
  path: string;
  bytes: number;
  isDir: boolean;
};

export type LargeFileEntry = {
  name: string;
  path: string;
  bytes: number;
};

export type DevArtifactEntry = {
  name: string;
  path: string;
  bytes: number;
};

export type SpaceAnalysisReport = {
  scannedPath: string;
  parentPath: string | null;
  totalBytes: number;
  scannedFiles: number;
  scannedDirs: number;
  truncated: boolean;
  children: SpaceNode[];
  largestFiles: LargeFileEntry[];
  devArtifacts: DevArtifactEntry[];
};
