import { useEffect, useRef, type Dispatch, type SetStateAction } from 'react';
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
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!showColumnMenu) return;
    const dismiss = (event: PointerEvent) => { if (!menuRef.current?.contains(event.target as Node)) setShowColumnMenu(false); };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') setShowColumnMenu(false); };
    document.addEventListener('pointerdown', dismiss);
    document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', dismiss); document.removeEventListener('keydown', escape); };
  }, [showColumnMenu, setShowColumnMenu]);
  return (
    <div className="section-title">
      <div>
        <h1>Hoạt động theo Process</h1>
        <span>Phiên quan sát hiện tại</span>
      </div>
      <div className="process-actions">
        <div className="search-box">
          <Search size={13} className="search-icon" />
          <input
            type="text"
            placeholder="Tìm process..."
            aria-label="Tìm process theo tên hoặc PID"
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
            aria-label="Số nhóm mỗi trang"
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

        <div className="dropdown-container" ref={menuRef}>
          <button className="icon-button-text" aria-expanded={showColumnMenu} onClick={() => setShowColumnMenu(!showColumnMenu)}>
            <SlidersHorizontal size={14} /> Cột hiển thị
          </button>
          {showColumnMenu && (
            <div className="column-menu-dropdown">
              <h3>HIỂN THỊ CỘT</h3>
              <div className="column-checkboxes">
                {([
                  { key: 'processCount', label: 'Process đang chạy' },
                  { key: 'memory', label: 'Bộ nhớ RAM' },
                  { key: 'uptime', label: 'Theo dõi tối đa trong nhóm' },
                  { key: 'netDown', label: 'Tải xuống' },
                  { key: 'netUp', label: 'Tải lên' },
                  { key: 'diskRead', label: 'I/O đọc' },
                  { key: 'diskWrite', label: 'I/O ghi' },
                  { key: 'totalIO', label: 'I/O phiên' },
                  { key: 'totalNet', label: 'Mạng phiên' },
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
