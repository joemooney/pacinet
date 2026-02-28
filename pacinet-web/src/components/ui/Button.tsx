interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
  size?: 'sm' | 'md';
}

const variants: Record<string, string> = {
  primary: 'bg-accent hover:bg-accent-hover text-white',
  secondary: 'bg-surface-hover hover:bg-edge text-content border border-edge',
  ghost: 'hover:bg-surface-hover text-content-secondary hover:text-content',
  danger: 'bg-red-500/20 hover:bg-red-500/30 text-red-400',
};

const sizes: Record<string, string> = {
  sm: 'px-3 py-1.5 text-xs',
  md: 'px-4 py-2 text-sm',
};

export default function Button({
  variant = 'primary',
  size = 'md',
  className = '',
  ...props
}: ButtonProps) {
  return (
    <button
      className={`rounded-lg font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${variants[variant]} ${sizes[size]} ${className}`}
      {...props}
    />
  );
}
