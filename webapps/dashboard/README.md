# Tunnel Dashboard

Web-based dashboard for managing and monitoring tunnel connections.

## Tech Stack

- **Framework**: React 19 + TypeScript
- **Build Tool**: Vite 7
- **Package Manager**: Bun
- **Styling**: Tailwind CSS v4
- **API Client**: @hey-api/openapi-ts (auto-generated from OpenAPI spec)

## Prerequisites

- [Bun](https://bun.sh/) 1.3+
- Backend server running on `http://localhost:8080` (for API proxy)

## Development

### Install Dependencies

```bash
bun install
```

### Generate API Client

The dashboard uses auto-generated TypeScript client from the backend OpenAPI specification:

```bash
# Generate API client from backend OpenAPI spec
bun run generate:api
```

This will fetch the OpenAPI spec from `http://localhost:8080/api/openapi.json` and generate type-safe API clients in `src/api/generated/`.

**Note**: Make sure the backend server is running before generating the API client.

### Start Development Server

```bash
bun run dev
```

The dashboard will be available at `http://localhost:3000`.

API requests to `/api/*` are automatically proxied to `http://localhost:8080` in development.

### Type Checking

```bash
bun run type-check
```

### Linting

```bash
bun run lint
```

### Build for Production

```bash
bun run build
```

The built assets will be in the `dist/` directory.

### Preview Production Build

```bash
bun run preview
```

## Project Structure

```
src/
├── api/
│   ├── generated/     # Auto-generated API client (don't edit manually)
│   └── client.ts      # API client configuration
├── components/        # React components
├── hooks/             # Custom React hooks
├── types/             # TypeScript type definitions
├── App.tsx            # Main app component
├── main.tsx           # Entry point
└── index.css          # Tailwind CSS imports
```

## API Integration

### Generating the API Client

1. Ensure backend is running with OpenAPI documentation at `/api/openapi.json`
2. Run `bun run generate:api`
3. Import and use the generated clients in your components

Example usage:

```typescript
import { TunnelService } from './api/generated';

// List tunnels
const tunnels = await TunnelService.listTunnels();

// Create tunnel
const tunnel = await TunnelService.createTunnel({
  requestBody: {
    protocol: 'http',
    local_port: 3000,
  }
});
```

### Configuring the API Client

Edit `openapi-ts.config.ts` to change the API spec URL or output settings.

## Styling with Tailwind CSS v4

This project uses Tailwind CSS v4 with the Vite plugin. Simply use Tailwind utility classes in your JSX:

```tsx
<div className="flex items-center gap-4 p-6 bg-white rounded-lg shadow">
  <h1 className="text-2xl font-bold text-gray-900">Dashboard</h1>
</div>
```

No need for traditional `tailwind.config.js` - Tailwind v4 uses CSS-based configuration.

## Environment Variables

Create a `.env.local` file for local development:

```env
VITE_API_URL=http://localhost:8080
```

Access in code:

```typescript
const apiUrl = import.meta.env.VITE_API_URL;
```

## Contributing

When adding new features:

1. Generate types from the backend OpenAPI spec
2. Use TypeScript strictly (no `any` types)
3. Follow the component structure in `src/components/`
4. Use Tailwind for styling (no custom CSS unless necessary)
5. Run type checking and linting before committing

## Deployment

The built dashboard can be:

1. **Embedded in Rust binary**: Build assets are bundled using `include_bytes!`
2. **Served separately**: Deploy `dist/` to any static hosting (Vercel, Netlify, etc.)
3. **Docker**: Serve with nginx or caddy

See the main repository documentation for deployment instructions.
