import type { CSSProperties, MouseEvent } from 'react';
import { ChevronDown, ChevronRight, X } from 'lucide-react';
import { formatBytes, formatDuration, formatRate } from '../../lib/format';
import type { ProcessGroup } from '../../types';

type ProcessRowProps = {
  group: ProcessGroup;
  gridStyle: CSSProperties;
  visibleColumns: Record<string, boolean>;
  isExpanded: boolean;
  toggleGroup: (name: string) => void;
  handleKillProcess: (event: MouseEvent, pidStr: string, name: string) => Promise<void>;
};

export function ProcessRow({
  group,
  gridStyle,
  visibleColumns,
  isExpanded,
  toggleGroup,
  handleKillProcess,
}: ProcessRowProps) {
  return (
    <div className="process-group-container">
      {/* Parent Row */}
      <div
        className={`process-row parent ${group.isGroup ? 'has-children' : ''}`}
        style={gridStyle}
        onClick={() => group.isGroup && toggleGroup(group.name)}
      >
        <span className="process-name">
          <span className="name-container">
            {group.isGroup && (
              <span className="expand-indicator">
                {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
              </span>
            )}
            <b>{group.name}</b>
            {!group.isGroup && (
              <button
                className="kill-process-btn"
                title={`Đóng tiến trình ${group.name}`}
                onClick={e => handleKillProcess(e, group.id, group.name)}
              >
                <X size={11} />
              </button>
            )}
          </span>
          <small>{group.pid}</small>
        </span>
        {visibleColumns.processCount && (
          <span className="process-count-badge">{group.children.length || 1}</span>
        )}
        {visibleColumns.memory && <span>{formatBytes(group.memoryBytes)}</span>}
        {visibleColumns.uptime && <span>{formatDuration(group.uptimeSecs)}</span>}
        {visibleColumns.netDown && <span className="down">{formatRate(group.networkDownBps)}</span>}
        {visibleColumns.netUp && <span className="up">{formatRate(group.networkUpBps)}</span>}
        {visibleColumns.diskRead && <span className="read">{formatRate(group.diskReadBps)}</span>}
        {visibleColumns.diskWrite && <span className="write">{formatRate(group.diskWriteBps)}</span>}
        {visibleColumns.totalIO && <span>{formatBytes(group.sessionDiskRead + group.sessionDiskWrite)}</span>}
      </div>

      {/* Child Rows */}
      {group.isGroup &&
        isExpanded &&
        group.children.map(child => (
          <div className="process-row child" style={gridStyle} key={child.pid}>
            <span className="process-name child-indent">
              <span className="name-container">
                <b>{child.name}</b>
                <button
                  className="kill-process-btn"
                  title={`Đóng tiến trình ${child.name}`}
                  onClick={e => handleKillProcess(e, child.id, child.name)}
                >
                  <X size={11} />
                </button>
              </span>
              <small>{child.pid}</small>
            </span>
            {visibleColumns.processCount && <span className="process-count-badge child-count">1</span>}
            {visibleColumns.memory && <span>{formatBytes(child.memoryBytes)}</span>}
            {visibleColumns.uptime && <span>{formatDuration(child.uptimeSecs)}</span>}
            {visibleColumns.netDown && <span className="down">{formatRate(child.networkDownBps)}</span>}
            {visibleColumns.netUp && <span className="up">{formatRate(child.networkUpBps)}</span>}
            {visibleColumns.diskRead && <span className="read">{formatRate(child.diskReadBps)}</span>}
            {visibleColumns.diskWrite && <span className="write">{formatRate(child.diskWriteBps)}</span>}
            {visibleColumns.totalIO && <span>{formatBytes(child.sessionDiskRead + child.sessionDiskWrite)}</span>}
          </div>
        ))}
    </div>
  );
}
