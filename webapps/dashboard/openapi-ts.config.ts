import { defineConfig } from '@hey-api/openapi-ts';

export default defineConfig({
  client: {
    bundle: true,
    name: '@hey-api/client-fetch',
    baseUrl: '', // Use relative paths (same origin)
  },
  input: 'http://localhost:9090/api-docs/openapi.json',
  output: {
    path: './src/api/generated',
    format: 'prettier',
    lint: 'eslint',
  },
  types: {
    enums: 'javascript',
  },
  services: {
    asClass: true,
  },
});
