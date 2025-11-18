export default {
  client: '@hey-api/client-fetch',
  input: 'http://localhost:13080/api/openapi.json',
  output: 'src/api/client',
  plugins: ['@tanstack/react-query'],
};
