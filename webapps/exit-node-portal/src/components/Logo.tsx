interface LogoProps {
  className?: string;
  size?: 'sm' | 'md' | 'lg';
}

export function Logo({ className = '', size = 'md' }: LogoProps) {
  const sizeClasses = {
    sm: 'h-6',
    md: 'h-8',
    lg: 'h-12',
  };

  return (
    <svg
      viewBox="0 0 120 32"
      className={`${sizeClasses[size]} ${className}`}
      fill="currentColor"
      xmlns="http://www.w3.org/2000/svg"
    >
      <text
        x="0"
        y="24"
        fontFamily="Inter, system-ui, sans-serif"
        fontWeight="800"
        fontSize="28"
      >
        localup
      </text>
    </svg>
  );
}

interface LogoIconProps {
  className?: string;
  size?: number;
}

export function LogoIcon({ className = '', size = 32 }: LogoIconProps) {
  return (
    <svg
      viewBox="0 0 32 32"
      width={size}
      height={size}
      className={className}
      fill="currentColor"
      xmlns="http://www.w3.org/2000/svg"
    >
      <text
        x="50%"
        y="50%"
        dominantBaseline="central"
        textAnchor="middle"
        fontFamily="Inter, system-ui, sans-serif"
        fontWeight="800"
        fontSize="24"
      >
        L
      </text>
    </svg>
  );
}
