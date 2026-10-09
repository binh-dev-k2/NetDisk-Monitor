import type { ProcessGroup } from '../types';

const HEADERS = [
  'Tên tiến trình',
  'PID',
  'Số process đang chạy',
  'Bộ nhớ RAM (MB)',
  'Theo dõi tối đa trong nhóm (Giây)',
  'Tải xuống (Bps)',
  'Tải lên (Bps)',
  'I/O đọc (Bps)',
  'I/O ghi (Bps)',
  'I/O phiên (Bytes)',
  'Mạng phiên đã đo (Bytes)',
];

function spreadsheetText(value: string): string {
  const safe = /^[\t\r\n]|^[\s]*[=+\-@]/.test(value) ? `'${value}` : value;
  return `"${safe.replace(/"/g, '""')}"`;
}

export function buildProcessCsv(groups: ProcessGroup[]): string {
  const rows = groups.map(group => [
    spreadsheetText(group.name),
    `"${group.children.map(child => child.pid).join(' | ')}"`,
    group.activeCount,
    (group.memoryBytes / (1024 * 1024)).toFixed(2),
    group.uptimeSecs,
    group.networkAvailable ? group.networkDownBps ?? 'N/A' : 'N/A',
    group.networkAvailable ? group.networkUpBps ?? 'N/A' : 'N/A',
    group.diskReadBps,
    group.diskWriteBps,
    group.sessionDiskRead + group.sessionDiskWrite,
    group.sessionNetworkDown != null && group.sessionNetworkUp != null ? group.sessionNetworkDown + group.sessionNetworkUp : 'N/A',
  ]);

  return '\uFEFF' + [HEADERS.join(','), ...rows.map(row => row.join(','))].join('\r\n');
}
