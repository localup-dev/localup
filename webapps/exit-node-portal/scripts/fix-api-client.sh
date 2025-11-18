#!/bin/bash
# Fix the generated API client to use relative paths instead of absolute URLs

CLIENT_FILE="src/api/client/client.gen.ts"

if [ -f "$CLIENT_FILE" ]; then
  # Replace the hardcoded baseUrl with empty string
  sed -i '' "s|baseUrl: 'http://localhost:13080'|baseUrl: '' // Use relative paths to leverage Vite proxy|g" "$CLIENT_FILE"
  echo "✅ Fixed API client baseUrl to use relative paths"
else
  echo "❌ Client file not found: $CLIENT_FILE"
  exit 1
fi
