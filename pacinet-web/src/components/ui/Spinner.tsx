export default function Spinner({ className = '' }: { className?: string }) {
  return (
    <div className={`flex items-center justify-center py-8 ${className}`}>
      <div className="w-6 h-6 border-2 border-edge border-t-accent rounded-full animate-spin" />
    </div>
  );
}
