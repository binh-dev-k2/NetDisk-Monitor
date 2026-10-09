import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { buildProcessCsv } from '../lib/csv';
import { groupProcesses, processActionTarget } from '../lib/process-model';
import { errorMessage } from '../lib/tauri-stream';
import { sortProcessGroups, type SortColumn } from '../lib/process-sort';
import type { ProcessMetric } from '../types';

export function useProcessTable(processes: ProcessMetric[]) {
  const [searchTerm, setSearchTerm] = useState('');
  const [pageSize, setPageSize] = useState(20);
  const [currentPage, setCurrentPage] = useState(1);
  const [showColumnMenu, setShowColumnMenu] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({});
  const [sortColumn, setSortColumn] = useState<SortColumn>('name');
  const [sortDirection, setSortDirection] = useState<'asc' | 'desc'>('asc');
  const [visibleColumns, setVisibleColumns] = useState<Record<string, boolean>>({
    name: true, processCount: true, memory: true, uptime: true, netDown: true,
    netUp: true, diskRead: true, diskWrite: true, totalIO: true, totalNet: true,
  });
  const [actionError, setActionError] = useState<string | null>(null);
  const [exportNotice, setExportNotice] = useState<string | null>(null);
  const [pendingKill, setPendingKill] = useState<string | null>(null);
  const actionPending = useRef(false);
  const handleSort = useCallback((key: SortColumn) => {
    if (sortColumn === key) setSortDirection(prev => prev === 'asc' ? 'desc' : 'asc');
    else { setSortColumn(key); setSortDirection('asc'); }
  }, [sortColumn]);
  const toggleGroup = useCallback((name: string) => setExpandedGroups(prev => ({ ...prev, [name]: !prev[name] })), []);
  const groupedProcesses = useMemo(() => groupProcesses(processes), [processes]);
  const filteredGroups = useMemo(() => {
    const query = searchTerm.toLowerCase().trim();
    return !query ? groupedProcesses : groupedProcesses.filter(group =>
      group.name.toLowerCase().includes(query) || group.children.some(child => `pid ${child.pid}`.includes(query)));
  }, [groupedProcesses, searchTerm]);
  const sortedGroups = useMemo(() => sortProcessGroups(filteredGroups, sortColumn, sortDirection), [filteredGroups, sortColumn, sortDirection]);
  const totalPages = Math.ceil(sortedGroups.length / pageSize) || 1;
  const clampedPage = Math.min(currentPage, totalPages);
  useEffect(() => setCurrentPage(page => Math.min(page, totalPages)), [totalPages]);
  const paginatedGroups = useMemo(() => sortedGroups.slice((clampedPage - 1) * pageSize, clampedPage * pageSize), [sortedGroups, clampedPage, pageSize]);
  const activeColumns = useMemo(() => {
    const columns: Array<{ key: SortColumn; label: string; width: string }> = [
      { key: 'name', label: 'Ứng dụng', width: 'minmax(160px, 1.8fr)' },
      { key: 'processCount', label: 'Đang chạy', width: 'minmax(70px, .7fr)' },
      { key: 'memory', label: 'RAM', width: 'minmax(85px, .8fr)' },
      { key: 'uptime', label: 'Theo dõi (max)', width: 'minmax(120px, 1fr)' },
      { key: 'netDown', label: 'Mạng ↓', width: 'minmax(85px, .8fr)' },
      { key: 'netUp', label: 'Mạng ↑', width: 'minmax(85px, .8fr)' },
      { key: 'diskRead', label: 'I/O đọc', width: 'minmax(105px, 1fr)' },
      { key: 'diskWrite', label: 'I/O ghi', width: 'minmax(105px, 1fr)' },
      { key: 'totalIO', label: 'I/O phiên', width: 'minmax(105px, 1fr)' },
      { key: 'totalNet', label: 'Mạng phiên', width: 'minmax(105px, 1fr)' },
    ];
    return columns.filter(column => column.key === 'name' || visibleColumns[column.key]);
  }, [visibleColumns]);
  const gridStyle = useMemo(() => ({ gridTemplateColumns: activeColumns.map(column => column.width).join(' ') }), [activeColumns]);
  const exportCSV = useCallback(async () => {
    if (!sortedGroups.length || actionPending.current) return;
    actionPending.current = true;
    setActionError(null);
    try {
      const filename = `NetDisk_Monitor_Report_${new Date().toISOString().slice(0, 10)}.csv`;
      const path = await invoke<string>('export_csv_file', { filename, content: buildProcessCsv(sortedGroups) });
      setExportNotice(`Đã lưu CSV: ${path}`);
    } catch (error) { setActionError(errorMessage(error)); }
    finally { actionPending.current = false; }
  }, [sortedGroups]);
  const handleKillProcess = useCallback(async (event: MouseEvent, process: ProcessMetric) => {
    event.stopPropagation();
    const target = processActionTarget(process);
    if (!target || actionPending.current) return;
    if (!confirm(`Đóng ${process.name} (PID ${process.pid})?`)) return;
    actionPending.current = true;
    setPendingKill(process.identity);
    setActionError(null);
    try { await invoke('kill_process', target); }
    catch (error) { setActionError(errorMessage(error)); }
    finally { actionPending.current = false; setPendingKill(null); }
  }, []);
  return { searchTerm, setSearchTerm, pageSize, setPageSize, currentPage, setCurrentPage,
    showColumnMenu, setShowColumnMenu, visibleColumns, setVisibleColumns, activeColumns, gridStyle,
    sortColumn, sortDirection, handleSort, sortedGroups, paginatedGroups, totalPages, clampedPage,
    expandedGroups, toggleGroup, exportCSV, handleKillProcess, actionError, exportNotice, pendingKill };
}
