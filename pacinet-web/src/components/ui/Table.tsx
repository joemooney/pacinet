import { useState, useCallback } from 'react';
import { ChevronUp, ChevronDown } from 'lucide-react';

interface TableProps {
  headers: string[];
  children: React.ReactNode;
  className?: string;
  sortable?: boolean;
  onSort?: (column: string, direction: 'asc' | 'desc') => void;
}

export default function Table({ headers, children, className = '', sortable = false, onSort }: TableProps) {
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc');

  const handleSort = useCallback(
    (header: string) => {
      if (!sortable || !onSort) return;
      const newDir = sortCol === header && sortDir === 'asc' ? 'desc' : 'asc';
      setSortCol(header);
      setSortDir(newDir);
      onSort(header, newDir);
    },
    [sortable, onSort, sortCol, sortDir]
  );

  return (
    <div className={`overflow-x-auto ${className}`}>
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-edge">
            {headers.map((h) => (
              <th
                key={h}
                onClick={() => handleSort(h)}
                className={`px-4 py-3 text-left text-xs font-medium text-content-muted uppercase tracking-wider ${
                  sortable && onSort ? 'cursor-pointer select-none hover:text-content-secondary' : ''
                }`}
              >
                <span className="inline-flex items-center gap-1">
                  {h}
                  {sortable && sortCol === h && (
                    sortDir === 'asc' ? <ChevronUp size={12} /> : <ChevronDown size={12} />
                  )}
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-edge">{children}</tbody>
      </table>
    </div>
  );
}
