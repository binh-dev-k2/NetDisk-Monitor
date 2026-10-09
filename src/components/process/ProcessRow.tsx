import { memo, type CSSProperties, type MouseEvent } from 'react';
import { ChevronDown, ChevronRight, X } from 'lucide-react';
import { formatBytes, formatDuration, formatRate } from '../../lib/format';
import { processNetworkText, sameProcessMetrics } from '../../lib/process-model';
import type { ProcessGroup, ProcessMetric } from '../../types';

const networkUnavailable = 'Lưu lượng mạng theo process hiện không khả dụng';

type ProcessRowProps = {
  group: ProcessGroup;
  gridStyle: CSSProperties;
  visibleColumns: Record<string, boolean>;
  isExpanded: boolean;
  toggleGroup: (name: string) => void;
  handleKillProcess: (event: MouseEvent, process: ProcessMetric) => Promise<void>;
  pendingKill: string | null;
};

export const ProcessRow = memo(function ProcessRow({ group, gridStyle, visibleColumns, isExpanded, toggleGroup, handleKillProcess, pendingKill }: ProcessRowProps) {
  const closeButton = (process: ProcessMetric) => process.active && (
    <button type="button" className="kill-process-btn" title={`Đóng ${process.name} (PID ${process.pid})`}
      aria-label={`Đóng ${process.name} (PID ${process.pid})`} disabled={pendingKill !== null}
      onClick={event => handleKillProcess(event, process)}><X size={11} /></button>
  );
  const metrics = (process: ProcessMetric | ProcessGroup, count: number) => <>
    {visibleColumns.processCount && <span className="process-count-badge">{count}</span>}
    {visibleColumns.memory && <span>{formatBytes(process.memoryBytes)}</span>}
    {visibleColumns.uptime && <span title={'children' in process ? 'Thời gian theo dõi dài nhất trong các process của nhóm' : undefined}>{formatDuration(process.uptimeSecs)}</span>}
    {visibleColumns.netDown && <span className="down" title={!process.networkAvailable || process.networkDownBps == null ? networkUnavailable : undefined}>{processNetworkText(process.networkDownBps, process.networkAvailable)}</span>}
    {visibleColumns.netUp && <span className="up" title={!process.networkAvailable || process.networkUpBps == null ? networkUnavailable : undefined}>{processNetworkText(process.networkUpBps, process.networkAvailable)}</span>}
    {visibleColumns.diskRead && <span className="read">{formatRate(process.diskReadBps)}</span>}
    {visibleColumns.diskWrite && <span className="write">{formatRate(process.diskWriteBps)}</span>}
    {visibleColumns.totalIO && <span>{formatBytes(process.sessionDiskRead + process.sessionDiskWrite)}</span>}
    {visibleColumns.totalNet && <span>{process.sessionNetworkDown != null && process.sessionNetworkUp != null ? formatBytes(process.sessionNetworkDown + process.sessionNetworkUp) : 'N/A'}</span>}
  </>;
  return <div className="process-group-container">
    <div className={`process-row parent${group.isGroup ? ' has-children' : ''}${!group.active ? ' inactive' : ''}`} style={gridStyle}
      onClick={() => group.isGroup && toggleGroup(group.name)}>
      <span className="process-name">
        <span className="name-container">
          {group.isGroup && <button type="button" className="expand-indicator" aria-label={`Mở nhóm ${group.name}`} aria-expanded={!!isExpanded}
            onClick={event => { event.stopPropagation(); toggleGroup(group.name); }}>
            {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>}
          <b title={group.name}>{group.name}</b>
          {!group.isGroup && closeButton(group.children[0])}
        </span>
        <small>{group.isGroup ? `${group.children.length} process` : `PID ${group.pid}`}{!group.active ? ' · Đã kết thúc' : ''}</small>
      </span>
      {metrics(group, group.activeCount)}
    </div>
    {group.isGroup && isExpanded && group.children.map(child => <div className={`process-row child${!child.active ? ' inactive' : ''}`} style={gridStyle} key={child.identity}>
      <span className="process-name child-indent">
        <span className="name-container"><b title={child.name}>{child.name}</b>{closeButton(child)}</span>
        <small>PID {child.pid}{!child.active ? ' · Đã kết thúc' : ''}</small>
      </span>
      {metrics(child, child.active ? 1 : 0)}
    </div>)}
  </div>;
}, (previous, next) => previous.group.id === next.group.id
  && sameProcessMetrics(previous.group.children, next.group.children)
  && previous.gridStyle === next.gridStyle && previous.visibleColumns === next.visibleColumns
  && previous.isExpanded === next.isExpanded && previous.pendingKill === next.pendingKill
  && previous.toggleGroup === next.toggleGroup && previous.handleKillProcess === next.handleKillProcess);
