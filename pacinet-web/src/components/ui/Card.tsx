interface CardProps {
  children: React.ReactNode;
  className?: string;
  title?: string;
}

export default function Card({ children, className = '', title }: CardProps) {
  return (
    <div className={`bg-surface-alt border border-edge rounded-xl ${className}`}>
      {title && (
        <div className="px-4 py-3 border-b border-edge">
          <h3 className="text-sm font-medium text-content-secondary">{title}</h3>
        </div>
      )}
      <div className="p-4">{children}</div>
    </div>
  );
}
