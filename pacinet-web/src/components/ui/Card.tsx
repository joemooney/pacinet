interface CardProps {
  children: React.ReactNode;
  className?: string;
  title?: string;
}

export default function Card({ children, className = '', title }: CardProps) {
  return (
    <div className={`rounded-2xl border border-edge/90 bg-surface-alt/90 shadow-[0_10px_30px_rgba(0,0,0,0.18)] backdrop-blur-sm ${className}`}>
      {title && (
        <div className="px-5 py-4 border-b border-edge/80">
          <h3 className="text-sm font-semibold tracking-wide text-content-secondary">{title}</h3>
        </div>
      )}
      <div className="p-5">{children}</div>
    </div>
  );
}
