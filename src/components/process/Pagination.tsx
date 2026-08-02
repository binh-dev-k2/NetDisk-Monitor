import type { Dispatch, SetStateAction } from 'react';

type PaginationProps = {
  clampedPage: number;
  totalPages: number;
  pageSize: number;
  totalCount: number;
  setCurrentPage: Dispatch<SetStateAction<number>>;
};

export function Pagination({
  clampedPage,
  totalPages,
  pageSize,
  totalCount,
  setCurrentPage,
}: PaginationProps) {
  if (totalCount === 0) return null;

  return (
    <div className="pagination">
      <div className="pagination-info">
        Hiển thị {Math.min((clampedPage - 1) * pageSize + 1, totalCount)} -{' '}
        {Math.min(clampedPage * pageSize, totalCount)} trong số {totalCount} nhóm
      </div>
      <div className="pagination-buttons">
        <button disabled={clampedPage === 1} onClick={() => setCurrentPage(1)}>
          Trang đầu
        </button>
        <button disabled={clampedPage === 1} onClick={() => setCurrentPage(prev => Math.max(1, prev - 1))}>
          Trước
        </button>
        <span className="page-indicator">
          Trang {clampedPage} / {totalPages}
        </span>
        <button disabled={clampedPage === totalPages} onClick={() => setCurrentPage(prev => Math.min(totalPages, prev + 1))}>
          Sau
        </button>
        <button disabled={clampedPage === totalPages} onClick={() => setCurrentPage(totalPages)}>
          Trang cuối
        </button>
      </div>
    </div>
  );
}
