interface Props {
  page: number;
  pageSize: number;
  total: number;
  onPageChange: (page: number) => void;
  onPageSizeChange: (pageSize: number) => void;
  pageSizeOptions?: number[];
}

export function PaginationControls({
  page,
  pageSize,
  total,
  onPageChange,
  onPageSizeChange,
  pageSizeOptions = [10, 25, 50],
}: Props) {
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const currentPage = Math.min(page, totalPages);
  const start = total === 0 ? 0 : (currentPage - 1) * pageSize + 1;
  const end = Math.min(currentPage * pageSize, total);

  return (
    <div className="mt-3 flex items-center justify-between gap-3 flex-wrap text-xs text-slate-400">
      <div>
        Showing {start}-{end} of {total}
      </div>
      <div className="flex items-center gap-3">
        <label className="flex items-center gap-2">
          Rows
          <select
            className="bg-slate-900 border border-slate-700 rounded px-2 py-1 text-slate-200"
            value={pageSize}
            onChange={(e) => onPageSizeChange(Number(e.target.value))}
          >
            {pageSizeOptions.map((opt) => (
              <option key={opt} value={opt}>
                {opt}
              </option>
            ))}
          </select>
        </label>
        <button
          className="px-2 py-1 rounded border border-slate-700 text-slate-300 disabled:opacity-40"
          onClick={() => onPageChange(currentPage - 1)}
          disabled={currentPage <= 1}
        >
          Prev
        </button>
        <span>
          {currentPage}/{totalPages}
        </span>
        <button
          className="px-2 py-1 rounded border border-slate-700 text-slate-300 disabled:opacity-40"
          onClick={() => onPageChange(currentPage + 1)}
          disabled={currentPage >= totalPages}
        >
          Next
        </button>
      </div>
    </div>
  );
}
