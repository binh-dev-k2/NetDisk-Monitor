import { useEffect, useState, useMemo, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Activity, ArrowDown, ArrowUp, ChevronDown, ChevronRight, Database, HardDrive, Network, RotateCcw, SlidersHorizontal, Timer, Wifi } from 'lucide-react';
import { formatBytes, formatDuration, formatRate } from './lib/format';
import { defaultOverlaySettings, normalizeOverlaySettings, type OverlaySettings } from './lib/overlay-settings';
import { sparklinePoints } from './lib/sparkline';

type ProcessMetric = { pid: string; name: string; memoryBytes: number; diskReadBps: number; diskWriteBps: number; sessionDiskRead: number; sessionDiskWrite: number; uptimeSecs: number; networkConnections: number; networkDownBps: number; networkUpBps: number };
type Snapshot = { networkDownBps: number; networkUpBps: number; diskReadBps: number; diskWriteBps: number; sessionNetworkDown: number; sessionNetworkUp: number; sessionDiskRead: number; sessionDiskWrite: number; sessionDurationSecs: number; processes: ProcessMetric[]; interfaces: string[] };
type History = Record<'down' | 'up' | 'read' | 'write', number[]>;
const empty: Snapshot = { networkDownBps: 0, networkUpBps: 0, diskReadBps: 0, diskWriteBps: 0, sessionNetworkDown: 0, sessionNetworkUp: 0, sessionDiskRead: 0, sessionDiskWrite: 0, sessionDurationSecs: 0, processes: [], interfaces: [] };
const emptyHistory: History = { down: [], up: [], read: [], write: [] };

function Sparkline({ values, tone }: { values: number[]; tone: string }) {
  const points = sparklinePoints(values, 240, 54);
  if (!points) return null;
  const fillPoints = `${points} 240,54 0,54`;
  return (
    <svg className={`sparkline ${tone}`} viewBox="0 0 240 54" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient id={`grad-${tone}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="currentColor" stopOpacity="0.22" />
          <stop offset="100%" stopColor="currentColor" stopOpacity="0.00" />
        </linearGradient>
      </defs>
      <polygon points={fillPoints} fill={`url(#grad-${tone})`} style={{ stroke: 'none' }} />
      <polyline points={points} />
    </svg>
  );
}

function MetricCard({ icon, label, value, tone, history }: { icon: ReactNode; label: string; value: string; tone: string; history: number[] }) {
  return <section className={`metric-card ${tone}`}><div className="metric-card-title"><span className="metric-icon">{icon}</span><span>{label}</span></div><strong>{value}</strong><Sparkline values={history} tone={tone} /><div className="metric-card-footer"><span>Đỉnh<b>{formatRate(Math.max(...history, 0))}</b></span><span>Hiện tại<b>{value}</b></span></div></section>;
}

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(empty);
  const [history, setHistory] = useState<History>(emptyHistory);
  const [tab, setTab] = useState<'overview' | 'processes'>('overview');
  const [overlaySettings, setOverlaySettings] = useState<OverlaySettings>(defaultOverlaySettings);
  const [taskbarMode, setTaskbarMode] = useState(false);
  const [overlayVisible, setOverlayVisible] = useState(true);

  // Process table enhancements states
  const [sessionNetwork, setSessionNetwork] = useState<Record<string, { down: number, up: number }>>({});
  const [visibleColumns, setVisibleColumns] = useState<Record<string, boolean>>({
    name: true,
    uptime: true,
    netDown: true,
    netUp: true,
    diskRead: true,
    diskWrite: true,
    totalIO: true,
    totalNet: true,
    connections: true,
  });
  const [showColumnMenu, setShowColumnMenu] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({});
  const [sortColumn, setSortColumn] = useState<string>('name');
  const [sortDirection, setSortDirection] = useState<'asc' | 'desc'>('asc');
  const [pageSize, setPageSize] = useState<number>(20);
  const [currentPage, setCurrentPage] = useState<number>(1);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const applySnapshot = (next: Snapshot) => {
      setSnapshot(next);
      setHistory(current => ({ down: [...current.down, next.networkDownBps].slice(-28), up: [...current.up, next.networkUpBps].slice(-28), read: [...current.read, next.diskReadBps].slice(-28), write: [...current.write, next.diskWriteBps].slice(-28) }));
      
      setSessionNetwork(current => {
        const nextSession = { ...current };
        next.processes.forEach(p => {
          const prev = nextSession[p.pid] || { down: 0, up: 0 };
          nextSession[p.pid] = {
            down: prev.down + p.networkDownBps,
            up: prev.up + p.networkUpBps,
          };
        });
        return nextSession;
      });
    };
    void invoke<Snapshot>('get_snapshot').then(applySnapshot).catch(console.error);
    void listen<Snapshot>('metrics://snapshot', event => applySnapshot(event.payload)).then(listener => { unlisten = listener; });
    return () => unlisten?.();
  }, []);

  useEffect(() => { void invoke<OverlaySettings>('get_overlay_settings').then(value => setOverlaySettings(normalizeOverlaySettings(value))).catch(console.error); }, []);
  useEffect(() => { void invoke<boolean>('get_taskbar_mode').then(setTaskbarMode).catch(console.error); }, []);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void invoke<boolean>('get_overlay_visible').then(setOverlayVisible).catch(console.error);
    void listen<boolean>('overlay://visibility', event => setOverlayVisible(event.payload)).then(listener => { unlisten = listener; });
    return () => unlisten?.();
  }, []);

  const updateOverlaySettings = (change: Partial<OverlaySettings>) => {
    const next = normalizeOverlaySettings({ ...overlaySettings, ...change });
    setOverlaySettings(next);
    void invoke<OverlaySettings>('update_overlay_settings', { settings: next }).then(value => setOverlaySettings(normalizeOverlaySettings(value))).catch(console.error);
  };
  const reset = () => {
    void invoke('reset_session_totals');
    setSessionNetwork({});
  };
  const toggleOverlayVisible = () => void invoke<boolean>('set_overlay_visible', { visible: !overlayVisible }).then(setOverlayVisible).catch(console.error);
  const toggleTaskbarMode = () => void invoke<boolean>('set_taskbar_mode', { enabled: !taskbarMode }).then(value => { setTaskbarMode(value); if (value) setOverlayVisible(true); }).catch(console.error);

  const toggleGroup = (name: string) => {
    setExpandedGroups(prev => ({ ...prev, [name]: !prev[name] }));
  };

  const handleSort = (colKey: string) => {
    if (sortColumn === colKey) {
      setSortDirection(prev => (prev === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortColumn(colKey);
      setSortDirection('asc');
    }
    setCurrentPage(1);
  };

  // Group processes by name
  const groupedProcesses = useMemo(() => {
    const groupsMap: Record<string, ProcessMetric[]> = {};
    snapshot.processes.forEach(p => {
      if (!groupsMap[p.name]) {
        groupsMap[p.name] = [];
      }
      groupsMap[p.name].push(p);
    });

    return Object.entries(groupsMap).map(([name, pList]) => {
      if (pList.length === 1) {
        const p = pList[0];
        const sessionNet = sessionNetwork[p.pid] || { down: 0, up: 0 };
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
          isGroup: false,
          children: [] as any[],
        };
      }

      const totalMemory = pList.reduce((acc, p) => acc + p.memoryBytes, 0);
      const totalDiskReadRate = pList.reduce((acc, p) => acc + p.diskReadBps, 0);
      const totalDiskWriteRate = pList.reduce((acc, p) => acc + p.diskWriteBps, 0);
      const totalSessionDiskRead = pList.reduce((acc, p) => acc + p.sessionDiskRead, 0);
      const totalSessionDiskWrite = pList.reduce((acc, p) => acc + p.sessionDiskWrite, 0);
      const maxUptime = pList.reduce((acc, p) => Math.max(acc, p.uptimeSecs), 0);
      const totalConnections = pList.reduce((acc, p) => acc + p.networkConnections, 0);
      const totalNetDownRate = pList.reduce((acc, p) => acc + p.networkDownBps, 0);
      const totalNetUpRate = pList.reduce((acc, p) => acc + p.networkUpBps, 0);
      
      const children = pList.map(p => {
        const sessionNet = sessionNetwork[p.pid] || { down: 0, up: 0 };
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
          isGroup: false,
        };
      });

      const totalSessionNetDown = children.reduce((acc, c) => acc + c.sessionNetDown, 0);
      const totalSessionNetUp = children.reduce((acc, c) => acc + c.sessionNetUp, 0);

      return {
        id: `group-${name}`,
        pid: `${pList.length} processes`,
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
        isGroup: true,
        children,
      };
    });
  }, [snapshot.processes, sessionNetwork]);

  // Sort groups
  const sortedGroups = useMemo(() => {
    if (!sortColumn) return groupedProcesses;

    const sorted = [...groupedProcesses];
    sorted.sort((left, right) => {
      let lVal: any = 0;
      let rVal: any = 0;

      switch (sortColumn) {
        case 'name':
          lVal = left.name.toLowerCase();
          rVal = right.name.toLowerCase();
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
        case 'connections':
          lVal = left.networkConnections;
          rVal = right.networkConnections;
          break;
        default:
          break;
      }

      if (lVal < rVal) return sortDirection === 'asc' ? -1 : 1;
      if (lVal > rVal) return sortDirection === 'asc' ? 1 : -1;
      return 0;
    });

    return sorted;
  }, [groupedProcesses, sortColumn, sortDirection]);

  const totalPages = Math.ceil(sortedGroups.length / pageSize) || 1;
  const clampedPage = Math.min(currentPage, totalPages);
  const paginatedGroups = useMemo(() => {
    return sortedGroups.slice((clampedPage - 1) * pageSize, clampedPage * pageSize);
  }, [sortedGroups, clampedPage, pageSize]);

  const activeColumns = useMemo(() => {
    const columnsConfig = [
      { key: 'name', label: 'Ứng dụng', width: '1.8fr' },
      { key: 'uptime', label: 'Đã theo dõi', width: '0.8fr' },
      { key: 'netDown', label: 'Tải xuống', width: '1fr' },
      { key: 'netUp', label: 'Tải lên', width: '1fr' },
      { key: 'diskRead', label: 'Đọc ổ đĩa', width: '0.9fr' },
      { key: 'diskWrite', label: 'Ghi ổ đĩa', width: '0.9fr' },
      { key: 'totalIO', label: 'Tổng I/O', width: '1fr' },
      { key: 'totalNet', label: 'Tổng Mạng', width: '1fr' },
      { key: 'connections', label: 'Kết nối', width: '0.7fr' },
    ];
    return columnsConfig.filter(col => col.key === 'name' || visibleColumns[col.key as keyof typeof visibleColumns]);
  }, [visibleColumns]);

  const gridStyle = useMemo(() => ({
    gridTemplateColumns: activeColumns.map(c => c.width).join(' '),
  }), [activeColumns]);

  return (
    <div className="dashboard-container">
      <aside className="sidebar">
        <div className="sidebar-brand">
          <Activity size={22} />
          <span>NetDisk Monitor</span>
        </div>

        <section className="sidebar-section">
          <h2>HIỂN THỊ WIDGET</h2>
          <div className="control-group">
            <label className="toggle">
              <input type="checkbox" checked={overlayVisible} disabled={taskbarMode} onChange={toggleOverlayVisible} />
              <span>Thanh nổi màn hình</span>
            </label>
            <label className="toggle">
              <input type="checkbox" checked={taskbarMode} onChange={toggleTaskbarMode} />
              <span>Ghim vào Taskbar</span>
            </label>
          </div>
        </section>

        <section className="sidebar-section">
          <h2>THÔNG SỐ HIỂN THỊ</h2>
          <div className="control-group checkboxes">
            {([
              { key: 'showNetworkDown', label: 'Tốc độ tải xuống' },
              { key: 'showNetworkUp', label: 'Tốc độ tải lên' },
              { key: 'showDiskRead', label: 'Tốc độ đọc đĩa' },
              { key: 'showDiskWrite', label: 'Tốc độ ghi đĩa' },
              { key: 'showSessionTotal', label: 'Tổng dung lượng phiên' },
              { key: 'showSessionDuration', label: 'Thời gian hoạt động' }
            ] as const).map(option => (
              <label className="toggle-check" key={option.key}>
                <input type="checkbox" checked={overlaySettings[option.key]} onChange={event => updateOverlaySettings({ [option.key]: event.target.checked })} />
                <span>{option.label}</span>
              </label>
            ))}
          </div>
        </section>

        <section className="sidebar-section">
          <div className="opacity-header">
            <h2>ĐỘ MỜ WIDGET</h2>
            <output>{overlaySettings.opacity}%</output>
          </div>
          <div className="opacity-slider-container">
            <input type="range" min="35" max="92" value={overlaySettings.opacity} onChange={event => updateOverlaySettings({ opacity: Number(event.target.value) })} />
          </div>
        </section>

        <div className="sidebar-footer">
          <button className="reset-button" onClick={reset}>
            <RotateCcw size={15} /> Reset Phiên
          </button>
        </div>
      </aside>

      <main className="content-area">
        <header className="content-header">
          <nav className="tabs" aria-label="Monitor views">
            <button className={tab === 'overview' ? 'active' : ''} onClick={() => setTab('overview')}>Tổng quan</button>
            <button className={tab === 'processes' ? 'active' : ''} onClick={() => setTab('processes')}>Theo Process</button>
          </nav>
          <div className="system-status">
            <span><i className="online" /> Đang giám sát</span>
            <span><Wifi size={14} /> Mạng: {snapshot.interfaces.length ? 'Đã kết nối' : 'Đang dò'}</span>
          </div>
        </header>

        {tab === 'overview' ? (
          <div className="overview-content">
            <section className="metric-grid">
              <MetricCard icon={<ArrowDown size={18} />} label="Tải xuống" value={formatRate(snapshot.networkDownBps)} tone="down" history={history.down} />
              <MetricCard icon={<ArrowUp size={18} />} label="Tải lên" value={formatRate(snapshot.networkUpBps)} tone="up" history={history.up} />
              <MetricCard icon={<HardDrive size={18} />} label="Đọc ổ đĩa" value={formatRate(snapshot.diskReadBps)} tone="read" history={history.read} />
              <MetricCard icon={<Database size={18} />} label="Ghi ổ đĩa" value={formatRate(snapshot.diskWriteBps)} tone="write" history={history.write} />
            </section>
            <section className="session-band">
              <div>
                <Network size={19} />
                <span>Network trong phiên<b>↓ {formatBytes(snapshot.sessionNetworkDown)} <em>↑ {formatBytes(snapshot.sessionNetworkUp)}</em></b></span>
              </div>
              <div>
                <HardDrive size={19} />
                <span>Disk trong phiên<b>↓ {formatBytes(snapshot.sessionDiskRead)} <em>↑ {formatBytes(snapshot.sessionDiskWrite)}</em></b></span>
              </div>
              <div>
                <Timer size={19} />
                <span>Thời gian phiên<b>{formatDuration(snapshot.sessionDurationSecs)}</b></span>
              </div>
            </section>
          </div>
        ) : (
          <section className="processes">
            <div className="section-title">
              <div>
                <h1>Hoạt động theo Process</h1>
                <span>Network là số kết nối TCP/UDP đang mở theo PID</span>
              </div>
              <div className="process-actions">
                <div className="page-size-selector">
                  <span>Dòng/trang:</span>
                  <select value={pageSize} onChange={e => { setPageSize(Number(e.target.value)); setCurrentPage(1); }}>
                    <option value={10}>10</option>
                    <option value={20}>20</option>
                    <option value={50}>50</option>
                    <option value={100}>100</option>
                  </select>
                </div>
                
                <div className="dropdown-container">
                  <button className="icon-button-text" onClick={() => setShowColumnMenu(!showColumnMenu)}>
                    <SlidersHorizontal size={14} /> Cột hiển thị
                  </button>
                  {showColumnMenu && (
                    <div className="column-menu-dropdown">
                      <h3>HIỂN THỊ CỘT</h3>
                      <div className="column-checkboxes">
                        {([
                          { key: 'uptime', label: 'Đã theo dõi' },
                          { key: 'netDown', label: 'Tải xuống' },
                          { key: 'netUp', label: 'Tải lên' },
                          { key: 'diskRead', label: 'Đọc ổ đĩa' },
                          { key: 'diskWrite', label: 'Ghi ổ đĩa' },
                          { key: 'totalIO', label: 'Tổng I/O' },
                          { key: 'totalNet', label: 'Tổng Mạng' },
                          { key: 'connections', label: 'Kết nối' }
                        ] as const).map(col => (
                          <label className="toggle-check" key={col.key}>
                            <input type="checkbox" checked={visibleColumns[col.key]} onChange={e => setVisibleColumns(prev => ({ ...prev, [col.key]: e.target.checked }))} />
                            <span>{col.label}</span>
                          </label>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
                <small>{sortedGroups.length} nhóm hiển thị</small>
              </div>
            </div>
            
            <div className="process-table">
              <div className="process-row headings" style={gridStyle}>
                {activeColumns.map(col => (
                  <span key={col.key} className={`sortable-header ${sortColumn === col.key ? 'active' : ''}`} onClick={() => handleSort(col.key)}>
                    {col.label}
                    {sortColumn === col.key && (
                      <span className="sort-arrow">
                        {sortDirection === 'asc' ? ' ↑' : ' ↓'}
                      </span>
                    )}
                  </span>
                ))}
              </div>
              
              {snapshot.processes.length ? paginatedGroups.map(group => {
                const isExpanded = expandedGroups[group.name];
                return (
                  <div key={group.id} className="process-group-container">
                    {/* Parent Row */}
                    <div className={`process-row parent ${group.isGroup ? 'has-children' : ''}`} style={gridStyle} onClick={() => group.isGroup && toggleGroup(group.name)}>
                      <span className="process-name">
                        <span className="name-container">
                          {group.isGroup && (
                            <span className="expand-indicator">
                              {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                            </span>
                          )}
                          <b>{group.name}</b>
                        </span>
                        <small>{group.pid} · RAM {formatBytes(group.memoryBytes)}</small>
                      </span>
                      {visibleColumns.uptime && <span>{formatDuration(group.uptimeSecs)}</span>}
                      {visibleColumns.netDown && <span className="down">{formatRate(group.networkDownBps)}</span>}
                      {visibleColumns.netUp && <span className="up">{formatRate(group.networkUpBps)}</span>}
                      {visibleColumns.diskRead && <span className="read">{formatRate(group.diskReadBps)}</span>}
                      {visibleColumns.diskWrite && <span className="write">{formatRate(group.diskWriteBps)}</span>}
                      {visibleColumns.totalIO && <span>{formatBytes(group.sessionDiskRead + group.sessionDiskWrite)}</span>}
                      {visibleColumns.totalNet && <span>{formatBytes(group.sessionNetDown + group.sessionNetUp)}</span>}
                      {visibleColumns.connections && <span className="connection-count"><Wifi size={14} />{group.networkConnections}</span>}
                    </div>

                    {/* Child Rows */}
                    {group.isGroup && isExpanded && group.children.map(child => (
                      <div className="process-row child" style={gridStyle} key={child.pid}>
                        <span className="process-name child-indent">
                          <b>{child.name}</b>
                          <small>{child.pid} · RAM {formatBytes(child.memoryBytes)}</small>
                        </span>
                        {visibleColumns.uptime && <span>{formatDuration(child.uptimeSecs)}</span>}
                        {visibleColumns.netDown && <span className="down">{formatRate(child.networkDownBps)}</span>}
                        {visibleColumns.netUp && <span className="up">{formatRate(child.networkUpBps)}</span>}
                        {visibleColumns.diskRead && <span className="read">{formatRate(child.diskReadBps)}</span>}
                        {visibleColumns.diskWrite && <span className="write">{formatRate(child.diskWriteBps)}</span>}
                        {visibleColumns.totalIO && <span>{formatBytes(child.sessionDiskRead + child.sessionDiskWrite)}</span>}
                        {visibleColumns.totalNet && <span>{formatBytes(child.sessionNetDown + child.sessionNetUp)}</span>}
                        {visibleColumns.connections && <span className="connection-count"><Wifi size={14} />{child.networkConnections}</span>}
                      </div>
                    ))}
                  </div>
                );
              }) : (
                <div className="empty">Đang thu thập dữ liệu process...</div>
              )}
              
              {sortedGroups.length > 0 && (
                <div className="pagination">
                  <span className="pagination-info">
                    Hiển thị {Math.min((clampedPage - 1) * pageSize + 1, sortedGroups.length)} - {Math.min(clampedPage * pageSize, sortedGroups.length)} trong số {sortedGroups.length} nhóm
                  </span>
                  <div className="pagination-buttons">
                    <button disabled={clampedPage === 1} onClick={() => setCurrentPage(1)}>Trang đầu</button>
                    <button disabled={clampedPage === 1} onClick={() => setCurrentPage(prev => Math.max(prev - 1, 1))}>Trước</button>
                    <span className="page-indicator">Trang {clampedPage} / {totalPages}</span>
                    <button disabled={clampedPage === totalPages} onClick={() => setCurrentPage(prev => Math.min(prev + 1, totalPages))}>Sau</button>
                    <button disabled={clampedPage === totalPages} onClick={() => setCurrentPage(totalPages)}>Trang cuối</button>
                  </div>
                </div>
              )}
            </div>
          </section>
        )}
      </main>
    </div>
  );
}
