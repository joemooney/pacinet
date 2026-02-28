interface TableProps {
  headers: string[];
  children: React.ReactNode;
  className?: string;
}

export default function Table({ headers, children, className = '' }: TableProps) {
  return (
    <div className={`overflow-x-auto ${className}`}>
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-edge">
            {headers.map((h) => (
              <th key={h} className="px-4 py-3 text-left text-xs font-medium text-content-muted uppercase tracking-wider">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-edge">{children}</tbody>
      </table>
    </div>
  );
}
