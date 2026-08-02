import type { Dispatch, SetStateAction } from 'react';
import { Download, Search, SlidersHorizontal, X } from 'lucide-react';

type ProcessTableHeaderProps = {
  searchTerm: string;
  setSearchTerm: (term: string) => void;
  pageSize: number;
  setPageSize: (size: number) => void;
  setCurrentPage: Dispatch<SetStateAction<number>>;
  showColumnMenu: boolean;
  setShowColumnMenu: (show: boolean) => void;
  visibleColumns: Record<string, boolean>;
  setVisibleColumns: Dispatch<SetStateAction<Record<string, boolean>>>;
  exportCSV: () => void;
  groupCount: number;
};

export function ProcessTableHeader({
  searchTerm,
  setSearchTerm,
  pageSize,
  setPageSize,
  setCurrentPage,
  showColumnMenu,
  setShowColumnMenu,
  visibleColumns,
  setVisibleColumns,
  exportCSV,
  groupCount,
}: ProcessTableHeaderProps) {
  return (
    <div className="section-title">
      <div>
        <h1>Hoạt động theo Process</h1>
        <span>Chi tiết dung lượng mạng và ổ đĩa theo tiến trình</span>
      </div>
      <div className="process-actions">
        <div className="search-box">
          <Search size={13} className="search-icon" />
          <input
            type="text"
            placeholder="Tìm process..."
            value={searchTerm}
            onChange={e => {
              setSearchTerm(e.target.value);
              setCurrentPage(1);
            }}
          />
          {searchTerm && (
            <button className="clear-search" onClick={() => setSearchTerm('')} title="Xóa tìm kiếm">
              <X size={12} />
            </button>
          )}
        </div>

        <div className="page-size-selector">
          <span>Dòng/trang:</span>
          <select
            value={pageSize}
            onChange={e => {
              setPageSize(Number(e.target.value));
              setCurrentPage(1);
            }}
          >
            <option value={10}>10</option>
            <option value={20}>20</option>
            <option value={50}>50</option>
            <option value={100}>100</option>
          </select>
        </div>

        <button className="icon-button-text" onClick={exportCSV} title="Xuất danh sách tiến trình ra file CSV">
          <Download size={14} /> Xuất CSV
        </button>

        <div className="dropdown-container">
          <button className="icon-button-text" onClick={() => setShowColumnMenu(!showColumnMenu)}>
            <SlidersHorizontal size={14} /> Cột hiển thị
          </button>
          {showColumnMenu && (
            <div className="column-menu-dropdown">
              <h3>HIỂN THỊ CỘT</h3>
              <div className="column-checkboxes">
                {([
                  { key: 'processCount', label: 'Số lượng Process' },
                  { key: 'memory', label: 'Bộ nhớ RAM' },
                  { key: 'uptime', label: 'Đã theo dõi' },
                  { key: 'netDown', label: 'Tải xuống' },
                  { key: 'netUp', label: 'Tải lên' },
                  { key: 'diskRead', label: 'Đọc ổ đĩa' },
                  { key: 'diskWrite', label: 'Ghi ổ đĩa' },
                  { key: 'totalIO', label: 'Tổng I/O' },
                ] as const).map(col => (
                  <label className="toggle-check" key={col.key}>
                    <input
                      type="checkbox"
                      checked={visibleColumns[col.key]}
                      onChange={e => setVisibleColumns(prev => ({ ...prev, [col.key]: e.target.checked }))}
                    />
                    <span>{col.label}</span>
                  </label>
                ))}
              </div>
            </div>
          )}
        </div>
        <small>{groupCount} nhóm hiển thị</small>
      </div>
    </div>
  );
}
