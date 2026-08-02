import { Pagination } from './process/Pagination';
import { ProcessRow } from './process/ProcessRow';
import { ProcessTableHeader } from './process/ProcessTableHeader';
import { useProcessTable } from '../hooks/useProcessTable';
import type { ProcessGroup } from '../types';

export type ProcessTabProps = ReturnType<typeof useProcessTable> & {
  processCount: number;
};

export function ProcessTab({
  searchTerm,
  setSearchTerm,
  pageSize,
  setPageSize,
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
  processCount,
}: ProcessTabProps) {
  return (
    <section className="processes">
      <ProcessTableHeader
        searchTerm={searchTerm}
        setSearchTerm={setSearchTerm}
        pageSize={pageSize}
        setPageSize={setPageSize}
        setCurrentPage={setCurrentPage}
        showColumnMenu={showColumnMenu}
        setShowColumnMenu={setShowColumnMenu}
        visibleColumns={visibleColumns}
        setVisibleColumns={setVisibleColumns}
        exportCSV={exportCSV}
        groupCount={sortedGroups.length}
      />

      <div className="process-table">
        <div className="process-row headings" style={gridStyle}>
          {activeColumns.map(col => (
            <span
              key={col.key}
              className={`sortable-header ${sortColumn === col.key ? 'active' : ''}`}
              onClick={() => handleSort(col.key)}
            >
              {col.label}
              {sortColumn === col.key && (
                <span className="sort-arrow">{sortDirection === 'asc' ? ' ↑' : ' ↓'}</span>
              )}
            </span>
          ))}
        </div>

        {processCount ? (
          paginatedGroups.map(group => (
            <ProcessRow
              key={group.id}
              group={group}
              gridStyle={gridStyle}
              visibleColumns={visibleColumns}
              isExpanded={expandedGroups[group.name]}
              toggleGroup={toggleGroup}
              handleKillProcess={handleKillProcess}
            />
          ))
        ) : (
          <div className="empty">Đang thu thập dữ liệu process...</div>
        )}
      </div>

      <Pagination
        clampedPage={clampedPage}
        totalPages={totalPages}
        pageSize={pageSize}
        totalCount={sortedGroups.length}
        setCurrentPage={setCurrentPage}
      />
    </section>
  );
}
