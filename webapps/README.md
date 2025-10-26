# Web Applications

This directory contains web-based user interfaces for the tunnel system.

## Applications

- **[dashboard/](./dashboard/)** - Main tunnel management dashboard

## Prerequisites

- [Bun](https://bun.sh/) 1.3+
- Backend server with OpenAPI documentation

## Quick Start

```bash
# Navigate to an application
cd dashboard

# Install dependencies
bun install

# Start development server
bun run dev
```

## Development Standards

All applications in this directory follow these standards:

### Tech Stack
- **Package Manager**: Bun
- **Framework**: React 19+ with TypeScript
- **Build Tool**: Vite 7+
- **Styling**: Tailwind CSS v4
- **API Client**: @hey-api/openapi-ts

### Project Structure
```
app-name/
├── src/
│   ├── api/generated/    # Auto-generated API client
│   ├── components/       # React components
│   ├── hooks/            # Custom hooks
│   ├── types/            # TypeScript types
│   ├── App.tsx           # Root component
│   └── index.css         # Tailwind CSS
├── openapi-ts.config.ts  # API generation config
├── vite.config.ts        # Vite configuration
└── package.json          # Dependencies
```

### Common Commands

```bash
# Development
bun run dev              # Start dev server (port 3000)
bun run generate:api     # Generate API client from OpenAPI spec
bun run type-check       # TypeScript type checking
bun run lint             # ESLint

# Production
bun run build            # Build for production
bun run preview          # Preview production build
```

## Backend Requirements

For the web apps to function, the Rust backend must:

1. Use `utoipa` with Axum 0.8+ for OpenAPI documentation
2. Expose OpenAPI spec at `/api/openapi.json`
3. Serve API on port 8080 (or configure proxy in vite.config.ts)
4. Configure CORS for development (allow localhost:3000)

## Adding a New Application

1. Create a new directory:
   ```bash
   cd webapps
   bun create vite my-app --template react-ts
   cd my-app
   ```

2. Install dependencies:
   ```bash
   bun install
   bun add -d @tailwindcss/vite@next tailwindcss@next @hey-api/openapi-ts
   bun add @hey-api/client-fetch
   ```

3. Configure Tailwind CSS v4 in `vite.config.ts`:
   ```typescript
   import tailwindcss from '@tailwindcss/vite'

   export default defineConfig({
     plugins: [react(), tailwindcss()],
   })
   ```

4. Update `src/index.css`:
   ```css
   @import "tailwindcss";
   ```

5. Create `openapi-ts.config.ts`:
   ```typescript
   import { defineConfig } from '@hey-api/openapi-ts';

   export default defineConfig({
     client: '@hey-api/client-fetch',
     input: 'http://localhost:8080/api/openapi.json',
     output: {
       path: './src/api/generated',
       format: 'prettier',
       lint: 'eslint',
     },
   });
   ```

6. Add scripts to `package.json`:
   ```json
   {
     "scripts": {
       "generate:api": "openapi-ts",
       "type-check": "tsc --noEmit"
     }
   }
   ```

7. Update this README with your new application

## Documentation

See [CLAUDE.md](../CLAUDE.md#web-applications) for detailed development guidelines and standards.

## Deployment

Applications can be deployed in several ways:

1. **Embedded in Rust binary** - Built assets bundled with `include_bytes!`
2. **Separate static hosting** - Deploy `dist/` to Vercel, Netlify, etc.
3. **Docker** - Multi-stage builds with frontend + backend

See individual application READMEs for specific deployment instructions.
