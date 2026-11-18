import { memo, type CSSProperties, type MouseEvent } from 'react';
import { AppWindow, ChevronDown, ChevronRight, FolderOpen, X } from 'lucide-react';
import { formatBytes, formatDuration, formatRate } from '../../lib/format';
import { processNetworkText, sameProcessMetrics } from '../../lib/process-model';
import type { ProcessGroup, ProcessMetric } from '../../types';

const networkUnavailable = 'Lưu lượng mạng theo process hiện không khả dụng';
const idleClass = (text: string) => (text === 'N/A' || text === '0 B/s' || text === '0.0 B/s' ? ' is-idle' : '');

type ProcessRowProps = {
  group: ProcessGroup;
  gridStyle: CSSProperties;
  visibleColumns: Record<string, boolean>;
  isExpanded: boolean;
  toggleGroup: (name: string) => void;
  handleKillProcess: (event: MouseEvent, process: ProcessMetric) => Promise<void>;
  handleRevealProcess?: (event: MouseEvent, process: ProcessMetric) => Promise<void>;
  pendingKill: string | null;
  iconUrl?: string;
};

export const ProcessRow = memo(function ProcessRow({ group, gridStyle, visibleColumns, isExpanded, toggleGroup, handleKillProcess, handleRevealProcess, pendingKill, iconUrl }: ProcessRowProps) {
  const rowActions = (process: ProcessMetric) => process.active && <>
    {handleRevealProcess && (
      <button type="button" className="reveal-process-btn" title={`Mở vị trí ${process.name} (PID ${process.pid})`}
        aria-label={`Mở vị trí ${process.name} (PID ${process.pid})`} disabled={pendingKill !== null}
        onClick={event => void handleRevealProcess(event, process)}><FolderOpen size={11} aria-hidden="true" /></button>
    )}
    <button type="button" className="kill-process-btn" title={`Đóng ${process.name} (PID ${process.pid})`}
      aria-label={`Đóng ${process.name} (PID ${process.pid})`} disabled={pendingKill !== null}
      onClick={event => void handleKillProcess(event, process)}><X size={11} aria-hidden="true" /></button>
  </>;
  const metrics = (process: ProcessMetric | ProcessGroup, count: number) => {
    const downText = processNetworkText(process.networkDownBps, process.networkAvailable);
    const upText = processNetworkText(process.networkUpBps, process.networkAvailable);
    const readText = formatRate(process.diskReadBps);
    const writeText = formatRate(process.diskWriteBps);
    return <>
      {visibleColumns.processCount && <span className="process-count-badge">{count}</span>}
      {visibleColumns.memory && <span>{formatBytes(process.memoryBytes)}</span>}
      {visibleColumns.uptime && <span title={'children' in process ? 'Thời gian theo dõi dài nhất trong các process của nhóm' : undefined}>{formatDuration(process.uptimeSecs)}</span>}
      {visibleColumns.netDown && <span className={`down${idleClass(downText)}`} title={!process.networkAvailable || process.networkDownBps == null ? networkUnavailable : undefined}>{downText}</span>}
      {visibleColumns.netUp && <span className={`up${idleClass(upText)}`} title={!process.networkAvailable || process.networkUpBps == null ? networkUnavailable : undefined}>{upText}</span>}
      {visibleColumns.diskRead && <span className={`read${idleClass(readText)}`}>{readText}</span>}
      {visibleColumns.diskWrite && <span className={`write${idleClass(writeText)}`}>{writeText}</span>}
      {visibleColumns.totalIO && <span>{formatBytes(process.sessionDiskRead + process.sessionDiskWrite)}</span>}
      {visibleColumns.totalNet && <span>{process.sessionNetworkDown != null && process.sessionNetworkUp != null ? formatBytes(process.sessionNetworkDown + process.sessionNetworkUp) : 'N/A'}</span>}
    </>;
  };
  return <div className="process-group-container">
    <div className={`process-row parent${group.isGroup ? ' has-children' : ''}${!group.active ? ' inactive' : ''}`} style={gridStyle}
      onClick={() => group.isGroup && toggleGroup(group.name)}>
      <span className="process-name">
        <span className="process-name-row">
          {group.isGroup && <button type="button" className="expand-indicator" aria-label={`Mở nhóm ${group.name}`} aria-expanded={!!isExpanded}
            onClick={event => { event.stopPropagation(); toggleGroup(group.name); }}>
            {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>}
          <span className="process-app-icon" aria-hidden="true">
            {iconUrl ? <img className="process-icon-img" src={iconUrl} alt="" width={18} height={18} /> : <AppWindow size={13} />}
          </span>
          <span className="process-name-text">
            <span className="name-container">
              <b title={group.name}>{group.name}</b>
              {!group.isGroup && rowActions(group.children[0])}
            </span>
            <small>{group.isGroup ? `${group.children.length} process` : `PID ${group.pid}`}{!group.active ? ' · Đã kết thúc' : ''}</small>
          </span>
        </span>
      </span>
      {metrics(group, group.activeCount)}
    </div>
    {group.isGroup && isExpanded && group.children.map(child => <div className={`process-row child${!child.active ? ' inactive' : ''}`} style={gridStyle} key={child.identity}>
      <span className="process-name child-indent">
        <span className="name-container"><b title={child.name}>{child.name}</b>{rowActions(child)}</span>
        <small>PID {child.pid}{!child.active ? ' · Đã kết thúc' : ''}</small>
      </span>
      {metrics(child, child.active ? 1 : 0)}
    </div>)}
  </div>;
}, (previous, next) => previous.group.id === next.group.id
  && sameProcessMetrics(previous.group.children, next.group.children)
  && previous.iconUrl === next.iconUrl
  && previous.gridStyle === next.gridStyle && previous.visibleColumns === next.visibleColumns
  && previous.isExpanded === next.isExpanded && previous.pendingKill === next.pendingKill
  && previous.toggleGroup === next.toggleGroup && previous.handleKillProcess === next.handleKillProcess
  && previous.handleRevealProcess === next.handleRevealProcess);
