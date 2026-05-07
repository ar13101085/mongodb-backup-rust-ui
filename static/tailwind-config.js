/* Sets the Tailwind CDN config. Loaded after the CDN script in every page. */
tailwind.config = {
  theme: {
    extend: {
      colors: {
        surface: '#ffffff',
        'surface-muted': '#f8fafc',
        'surface-sunken': '#f1f5f9',
        border: '#e2e8f0',
        'border-strong': '#cbd5e1',
        text: '#0f172a',
        'text-muted': '#64748b',
        'text-subtle': '#94a3b8',
        accent: { DEFAULT: '#059669', hover: '#047857', soft: '#ecfdf5' },
        success: '#10b981',
        warning: '#d97706',
        danger: '#dc2626',
      },
      fontFamily: {
        sans: ['Inter', 'ui-sans-serif', 'system-ui', 'sans-serif'],
        mono: ['"JetBrains Mono"', 'ui-monospace', 'monospace'],
      },
      boxShadow: {
        card: '0 1px 2px rgba(15,23,42,0.04), 0 1px 3px rgba(15,23,42,0.06)',
      },
    },
  },
};
