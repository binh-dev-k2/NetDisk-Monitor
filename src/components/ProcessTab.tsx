import { memo } from 'react';
import { Pagination } from './process/Pagination';
import { ProcessRow } from './process/ProcessRow';
import { ProcessTableHeader } from './process/ProcessTableHeader';
import { useProcessIcons } from '../hooks/useProcessIcons';
import { useProcessTable } from '../hooks/useProcessTable';
import type { ProcessMetric } from '../types';

export const ProcessTab = memo(function ProcessTab({ processes, hidden, loading, stale, error, privilegeNotice, onRetry }: {
  processes: ProcessMetric[]; hidden: boolean; loading: boolean; stale: boolean; error: string | null; privilegeNotice?: string | null; onRetry: () => void;
}) {
  const table = useProcessTable(processes);
  const icons = useProcessIcons(table.paginatedGroups, !hidden);
  return <section className="processes" hidden={hidden}>
    <ProcessTableHeader {...table} groupCount={table.sortedGroups.length} />
    <div className="process-retention" role="status">
      Phiên hiện tại · Giữ tối đa 512 process đã kết thúc; khi đầy, loại bản ghi được giữ lâu nhất. Không lưu process qua lần khởi động.
    </div>
    {privilegeNotice && <div className="notice" role="status">{privilegeNotice}</div>}
    {error && <div className="diagnostics diagnostics-row" role="alert">
      <span>{error}</span>
      <button type="button" className="inline-action-btn" onClick={onRetry}>Thử lại</button>
    </div>}
    {table.actionError && <div className="diagnostics" role="alert">{table.actionError}</div>}
    {table.exportNotice && <div className="notice" role="status">{table.exportNotice}</div>}
    {stale && <div className="diagnostics" role="status">Dữ liệu process chưa được cập nhật.</div>}
    <div className="process-table" role="region" tabIndex={0} aria-label="Danh sách process">
      <div className="process-table-inner" style={{ minWidth: table.activeColumns.length * 117 + 125 }}>
        <div className="process-row headings" style={table.gridStyle}>
          {table.activeColumns.map(column => <button type="button" key={column.key}
            className={`sortable-header${table.sortColumn === column.key ? ' active' : ''}`}
            onClick={() => table.handleSort(column.key)} aria-label={`${column.label}: sắp xếp`}>
            {column.label}{table.sortColumn === column.key && <span className="sort-arrow">{table.sortDirection === 'asc' ? ' ↑' : ' ↓'}</span>}
          </button>)}
        </div>
        {table.paginatedGroups.map(group => <ProcessRow key={group.id} group={group} gridStyle={table.gridStyle}
          visibleColumns={table.visibleColumns} isExpanded={table.expandedGroups[group.name]}
          toggleGroup={table.toggleGroup} handleKillProcess={table.handleKillProcess}
          handleRevealProcess={table.handleRevealProcess} pendingKill={table.pendingKill}
          iconUrl={icons[group.name]} />)}
      </div>
      {!table.paginatedGroups.length && <div className="empty">
        <p>{loading ? 'Đang thu thập dữ liệu process...' : table.searchTerm.trim() ? 'Không có process phù hợp.' : 'Không có dữ liệu process.'}</p>
        {!loading && table.searchTerm.trim() && (
          <button type="button" className="inline-action-btn" onClick={() => table.setSearchTerm('')}>Xóa tìm kiếm</button>
        )}
      </div>}
    </div>
    <Pagination clampedPage={table.clampedPage} totalPages={table.totalPages} pageSize={table.pageSize}
      totalCount={table.sortedGroups.length} setCurrentPage={table.setCurrentPage} />
  </section>;
});
