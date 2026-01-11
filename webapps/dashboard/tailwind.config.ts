import type { Config } from 'tailwindcss';

export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        dark: {
          bg: '#0a0e1a',
          surface: '#0f1523',
          'surface-light': '#1a1f2e',
          border: '#262d3e',
          text: {
            primary: '#e2e8f0',
            secondary: '#94a3b8',
            muted: '#64748b',
          },
        },
        accent: {
          blue: {
            DEFAULT: '#3b82f6',
            light: '#60a5fa',
            dark: '#2563eb',
          },
          green: {
            DEFAULT: '#10b981',
            light: '#34d399',
            dark: '#059669',
          },
          red: {
            DEFAULT: '#ef4444',
            light: '#f87171',
            dark: '#dc2626',
          },
          purple: {
            DEFAULT: '#8b5cf6',
            light: '#a78bfa',
            dark: '#7c3aed',
          },
        },
      },
      boxShadow: {
        'dark-sm': '0 1px 2px 0 rgba(0, 0, 0, 0.5)',
        'dark-md': '0 4px 6px -1px rgba(0, 0, 0, 0.5), 0 2px 4px -1px rgba(0, 0, 0, 0.3)',
        'dark-lg': '0 10px 15px -3px rgba(0, 0, 0, 0.5), 0 4px 6px -2px rgba(0, 0, 0, 0.3)',
        'glow-blue': '0 0 20px rgba(59, 130, 246, 0.3)',
        'glow-green': '0 0 20px rgba(16, 185, 129, 0.3)',
      },
    },
  },
} satisfies Config;
