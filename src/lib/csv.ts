import type { ProcessGroup } from '../types';

const HEADERS = [
  'Tên tiến trình',
  'PID',
  'Số sub-process',
  'Bộ nhớ RAM (MB)',
  'Thời gian (Giây)',
  'Tải xuống (Bps)',
  'Tải lên (Bps)',
  'Đọc ổ đĩa (Bps)',
  'Ghi ổ đĩa (Bps)',
  'Tổng I/O Đĩa (Bytes)',
];

export function buildProcessCsv(groups: ProcessGroup[]): string {
  const rows = groups.map(group => [
    `"${group.name.replace(/"/g, '""')}"`,
    `"${group.pid}"`,
    group.children.length || 1,
    (group.memoryBytes / (1024 * 1024)).toFixed(2),
    group.uptimeSecs,
    group.networkDownBps,
    group.networkUpBps,
    group.diskReadBps,
    group.diskWriteBps,
    group.sessionDiskRead + group.sessionDiskWrite,
  ]);

  return '\uFEFF' + [HEADERS.join(','), ...rows.map(row => row.join(','))].join('\r\n');
}
