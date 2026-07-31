import { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ProcessMetric } from '../types';

export function useProcessTable(
  processes: ProcessMetric[],
  sessionNetwork: Record<string, { down: number; up: number }>
) {
  const [searchTerm, setSearchTerm] = useState('');
  const [pageSize, setPageSize] = useState<number>(20);
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [showColumnMenu, setShowColumnMenu] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({});
  const [sortColumn, setSortColumn] = useState<string>('name');
  const [sortDirection, setSortDirection] = useState<'asc' | 'desc'>('asc');
  const [visibleColumns, setVisibleColumns] = useState<Record<string, boolean>>({
    name: true,
    processCount: true,
    memory: true,
    uptime: true,
    netDown: true,
    netUp: true,
    diskRead: true,
    diskWrite: true,
    totalIO: true,
  });

  const handleSort = (key: string) => {
    if (sortColumn === key) {
      setSortDirection(prev => (prev === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortColumn(key);
      setSortDirection('asc');
    }
  };

  const toggleGroup = (name: string) => {
    setExpandedGroups(prev => ({ ...prev, [name]: !prev[name] }));
  };

  const groupedProcesses = useMemo(() => {
    const map = new Map<string, ProcessMetric[]>();
    for (const p of processes) {
      const existing = map.get(p.name) || [];
      existing.push(p);
      map.set(p.name, existing);
    }

    return Array.from(map.entries()).map(([name, pList]) => {
      const totalMemory = pList.reduce((acc, c) => acc + c.memoryBytes, 0);
      const totalDiskReadRate = pList.reduce((acc, c) => acc + c.diskReadBps, 0);
      const totalDiskWriteRate = pList.reduce((acc, c) => acc + c.diskWriteBps, 0);
      const totalSessionDiskRead = pList.reduce((acc, c) => acc + c.sessionDiskRead, 0);
      const totalSessionDiskWrite = pList.reduce((acc, c) => acc + c.sessionDiskWrite, 0);
      const maxUptime = Math.max(...pList.map(c => c.uptimeSecs));
      const totalConnections = pList.reduce((acc, c) => acc + c.networkConnections, 0);
      const totalNetDownRate = pList.reduce((acc, c) => acc + c.networkDownBps, 0);
      const totalNetUpRate = pList.reduce((acc, c) => acc + c.networkUpBps, 0);

      const children = pList.map(p => {
        const pidKey = `${p.name}-${p.pid}`;
        const sessionNet = sessionNetwork[pidKey] || { down: 0, up: 0 };
        return {
          id: p.pid,
          pid: `PID ${p.pid}`,
          name: p.name,
          memoryBytes: p.memoryBytes,
          diskReadBps: p.diskReadBps,
          diskWriteBps: p.diskWriteBps,
          sessionDiskRead: p.sessionDiskRead,
          sessionDiskWrite: p.sessionDiskWrite,
          uptimeSecs: p.uptimeSecs,
          networkConnections: p.networkConnections,
          networkDownBps: p.networkDownBps,
          networkUpBps: p.networkUpBps,
          sessionNetDown: sessionNet.down,
          sessionNetUp: sessionNet.up,
        };
      });

      const totalSessionNetDown = children.reduce((acc, c) => acc + c.sessionNetDown, 0);
      const totalSessionNetUp = children.reduce((acc, c) => acc + c.sessionNetUp, 0);

      return {
        id: `group-${name}`,
        pid: `PID ${pList[0].pid}`,
        name,
        memoryBytes: totalMemory,
        diskReadBps: totalDiskReadRate,
        diskWriteBps: totalDiskWriteRate,
        sessionDiskRead: totalSessionDiskRead,
        sessionDiskWrite: totalSessionDiskWrite,
        uptimeSecs: maxUptime,
        networkConnections: totalConnections,
        networkDownBps: totalNetDownRate,
        networkUpBps: totalNetUpRate,
        sessionNetDown: totalSessionNetDown,
        sessionNetUp: totalSessionNetUp,
        isGroup: pList.length > 1,
        children,
      };
    });
  }, [processes, sessionNetwork]);

  const filteredGroups = useMemo(() => {
    if (!searchTerm.trim()) return groupedProcesses;
    const query = searchTerm.toLowerCase().trim();
    return groupedProcesses.filter(group => {
      const matchGroup = group.name.toLowerCase().includes(query) || group.pid.toLowerCase().includes(query);
      if (matchGroup) return true;
      return group.children.some(child => child.name.toLowerCase().includes(query) || child.pid.toLowerCase().includes(query));
    });
  }, [groupedProcesses, searchTerm]);

  const sortedGroups = useMemo(() => {
    if (!sortColumn) return filteredGroups;

    const sorted = [...filteredGroups];
    sorted.sort((left, right) => {
      let lVal: any = 0;
      let rVal: any = 0;

      switch (sortColumn) {
        case 'name':
          lVal = left.name.toLowerCase();
          rVal = right.name.toLowerCase();
          break;
        case 'processCount':
          lVal = left.children.length;
          rVal = right.children.length;
          break;
        case 'memory':
          lVal = left.memoryBytes;
          rVal = right.memoryBytes;
          break;
        case 'uptime':
          lVal = left.uptimeSecs;
          rVal = right.uptimeSecs;
          break;
        case 'netDown':
          lVal = left.networkDownBps;
          rVal = right.networkDownBps;
          break;
        case 'netUp':
          lVal = left.networkUpBps;
          rVal = right.networkUpBps;
          break;
        case 'diskRead':
          lVal = left.diskReadBps;
          rVal = right.diskReadBps;
          break;
        case 'diskWrite':
          lVal = left.diskWriteBps;
          rVal = right.diskWriteBps;
          break;
        case 'totalIO':
          lVal = left.sessionDiskRead + left.sessionDiskWrite;
          rVal = right.sessionDiskRead + right.sessionDiskWrite;
          break;
        case 'totalNet':
          lVal = left.sessionNetDown + left.sessionNetUp;
          rVal = right.sessionNetDown + right.sessionNetUp;
          break;
        default:
          lVal = left.name;
          rVal = right.name;
      }

      if (lVal < rVal) return sortDirection === 'asc' ? -1 : 1;
      if (lVal > rVal) return sortDirection === 'asc' ? 1 : -1;
      return 0;
    });

    return sorted;
  }, [filteredGroups, sortColumn, sortDirection]);

  const totalPages = Math.ceil(sortedGroups.length / pageSize) || 1;
  const clampedPage = Math.min(currentPage, totalPages);
  const paginatedGroups = useMemo(() => {
    return sortedGroups.slice((clampedPage - 1) * pageSize, clampedPage * pageSize);
  }, [sortedGroups, clampedPage, pageSize]);

  const activeColumns = useMemo(() => {
    const columnsConfig = [
      { key: 'name', label: 'Ứng dụng', width: '1.8fr' },
      { key: 'processCount', label: 'Số Process', width: '0.7fr' },
      { key: 'memory', label: 'RAM', width: '0.8fr' },
      { key: 'uptime', label: 'Đã theo dõi', width: '0.8fr' },
      { key: 'netDown', label: 'Tải xuống', width: '1fr' },
      { key: 'netUp', label: 'Tải lên', width: '1fr' },
      { key: 'diskRead', label: 'Đọc ổ đĩa', width: '0.9fr' },
      { key: 'diskWrite', label: 'Ghi ổ đĩa', width: '0.9fr' },
      { key: 'totalIO', label: 'Tổng I/O', width: '1fr' },
    ];
    return columnsConfig.filter(col => col.key === 'name' || visibleColumns[col.key as keyof typeof visibleColumns]);
  }, [visibleColumns]);

  const gridStyle = useMemo(() => ({
    gridTemplateColumns: activeColumns.map(c => c.width).join(' '),
  }), [activeColumns]);

  const exportCSV = async () => {
    if (!sortedGroups.length) return;
    const headers = [
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
    const rows = sortedGroups.map(group => [
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

    const csvString = '\uFEFF' + [headers.join(','), ...rows.map(e => e.join(','))].join('\r\n');
    const filename = `NetDisk_Monitor_Report_${new Date().toISOString().slice(0, 10)}.csv`;

    try {
      const savedPath = await invoke<string>('export_csv_file', { filename, content: csvString });
      alert(`Xuất CSV thành công!\n\nFile đã được lưu tại:\n${savedPath}\n\nThư mục chứa file đã tự động mở trong File Explorer.`);
    } catch (err: any) {
      alert(`Không thể xuất CSV: ${err}`);
    }
  };

  const handleKillProcess = async (event: React.MouseEvent, pidStr: string, name: string) => {
    event.stopPropagation();
    const purePid = pidStr.replace(/[^0-9]/g, '');
    if (!purePid) return;
    if (confirm(`Bạn có chắc muốn đóng tiến trình ${name} (PID: ${purePid}) không?`)) {
      try {
        await invoke('kill_process', { pid: purePid });
      } catch (err: any) {
        alert(err || 'Không thể đóng tiến trình này');
      }
    }
  };

  return {
    searchTerm,
    setSearchTerm,
    pageSize,
    setPageSize,
    currentPage,
    setCurrentPage,
    showColumnMenu,
    setShowColumnMenu,
    visibleColumns,
    setVisibleColumns,
    activeColumns,
    gridStyle,
    sortColumn,
    sortDirection,
    handleSort,
    sortedGroups,
    paginatedGroups,
    totalPages,
    clampedPage,
    expandedGroups,
    toggleGroup,
    exportCSV,
    handleKillProcess,
  };
}
