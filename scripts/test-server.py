#!/usr/bin/env python3
"""
Comprehensive test server for localup tunnel testing.

Supports:
  - All HTTP methods: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS
  - WebSocket (RFC 6455) with echo + broadcast
  - Server-Sent Events (SSE) streaming
  - JSON request/response inspection
  - File upload (multipart)
  - Chunked transfer encoding
  - CORS headers

Zero dependencies - uses only Python stdlib.

Usage:
  python3 scripts/test-server.py [--port 8080]
"""

import argparse
import hashlib
import base64
import json
import os
import selectors
import socket
import struct
import sys
import threading
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
from io import BytesIO
from datetime import datetime, timezone
from urllib.parse import urlparse, parse_qs

# ---------------------------------------------------------------------------
# WebSocket helpers (RFC 6455)
# ---------------------------------------------------------------------------
WS_MAGIC = b"258EAFA5-E914-47DA-95CA-5AB4C6E3AB10"

# Connected WebSocket clients: list of (socket, address)
ws_clients: list[tuple[socket.socket, str]] = []
ws_clients_lock = threading.Lock()


def ws_accept_key(key: str) -> str:
    digest = hashlib.sha1(key.encode() + WS_MAGIC).digest()
    return base64.b64encode(digest).decode()


def ws_decode_frame(data: bytes):
    """Decode a single WebSocket frame. Returns (opcode, payload, consumed)."""
    if len(data) < 2:
        return None, None, 0
    b0, b1 = data[0], data[1]
    opcode = b0 & 0x0F
    masked = b1 & 0x80
    length = b1 & 0x7F
    offset = 2
    if length == 126:
        if len(data) < 4:
            return None, None, 0
        length = struct.unpack("!H", data[2:4])[0]
        offset = 4
    elif length == 127:
        if len(data) < 10:
            return None, None, 0
        length = struct.unpack("!Q", data[2:10])[0]
        offset = 10
    if masked:
        if len(data) < offset + 4:
            return None, None, 0
        mask = data[offset : offset + 4]
        offset += 4
    if len(data) < offset + length:
        return None, None, 0
    payload = bytearray(data[offset : offset + length])
    if masked:
        for i in range(length):
            payload[i] ^= mask[i % 4]
    return opcode, bytes(payload), offset + length


def ws_encode_frame(opcode: int, payload: bytes) -> bytes:
    frame = bytearray()
    frame.append(0x80 | opcode)  # FIN + opcode
    length = len(payload)
    if length < 126:
        frame.append(length)
    elif length < 65536:
        frame.append(126)
        frame.extend(struct.pack("!H", length))
    else:
        frame.append(127)
        frame.extend(struct.pack("!Q", length))
    frame.extend(payload)
    return bytes(frame)


def ws_handle_client(conn: socket.socket, addr):
    """Handle a WebSocket connection after upgrade."""
    with ws_clients_lock:
        ws_clients.append((conn, addr))
    print(f"[WS] Client connected: {addr} (total: {len(ws_clients)})")

    buf = b""
    try:
        while True:
            chunk = conn.recv(4096)
            if not chunk:
                break
            buf += chunk
            while buf:
                opcode, payload, consumed = ws_decode_frame(buf)
                if opcode is None:
                    break
                buf = buf[consumed:]
                if opcode == 0x8:  # Close
                    close_frame = ws_encode_frame(0x8, payload[:2] if payload else b"")
                    try:
                        conn.sendall(close_frame)
                    except Exception:
                        pass
                    return
                elif opcode == 0x9:  # Ping
                    conn.sendall(ws_encode_frame(0xA, payload))  # Pong
                elif opcode == 0xA:  # Pong
                    pass
                elif opcode in (0x1, 0x2):  # Text / Binary
                    # Echo back to sender
                    try:
                        text = payload.decode("utf-8") if opcode == 0x1 else ""
                    except UnicodeDecodeError:
                        text = ""

                    echo_msg = json.dumps(
                        {
                            "type": "echo",
                            "data": text
                            if opcode == 0x1
                            else base64.b64encode(payload).decode(),
                            "timestamp": datetime.now(timezone.utc).isoformat(),
                            "clients_connected": len(ws_clients),
                        }
                    )
                    conn.sendall(ws_encode_frame(0x1, echo_msg.encode()))

                    # Broadcast to all other clients
                    broadcast_msg = json.dumps(
                        {
                            "type": "broadcast",
                            "from": str(addr),
                            "data": text
                            if opcode == 0x1
                            else base64.b64encode(payload).decode(),
                            "timestamp": datetime.now(timezone.utc).isoformat(),
                        }
                    )
                    broadcast_frame = ws_encode_frame(0x1, broadcast_msg.encode())
                    with ws_clients_lock:
                        for client, client_addr in ws_clients:
                            if client is not conn:
                                try:
                                    client.sendall(broadcast_frame)
                                except Exception:
                                    pass
    except (ConnectionResetError, BrokenPipeError, OSError):
        pass
    finally:
        with ws_clients_lock:
            ws_clients[:] = [(c, a) for c, a in ws_clients if c is not conn]
        try:
            conn.close()
        except Exception:
            pass
        print(f"[WS] Client disconnected: {addr} (total: {len(ws_clients)})")


# ---------------------------------------------------------------------------
# SSE counter (shared across connections)
# ---------------------------------------------------------------------------
sse_counter = 0
sse_counter_lock = threading.Lock()


# ---------------------------------------------------------------------------
# HTML dashboard
# ---------------------------------------------------------------------------
DASHBOARD_HTML = """\
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>localup Test Server</title>
<style>
  *, *::before, *::after { box-sizing: border-box; }
  body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, monospace;
         background: #0a0a0a; color: #e0e0e0; margin: 0; padding: 20px; }
  h1 { color: #00d4aa; margin-bottom: 4px; }
  .subtitle { color: #888; margin-bottom: 24px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; max-width: 1000px; }
  @media (max-width: 700px) { .grid { grid-template-columns: 1fr; } }
  .card { background: #151515; border: 1px solid #2a2a2a; border-radius: 8px; padding: 16px; }
  .card h2 { color: #00d4aa; font-size: 14px; margin: 0 0 12px; text-transform: uppercase; letter-spacing: 1px; }
  .log { background: #0d0d0d; border: 1px solid #222; border-radius: 4px; padding: 8px;
         font-size: 12px; height: 200px; overflow-y: auto; white-space: pre-wrap; word-break: break-all; }
  button { background: #00d4aa; color: #0a0a0a; border: none; border-radius: 4px;
           padding: 6px 14px; cursor: pointer; font-weight: 600; font-size: 13px; margin: 2px; }
  button:hover { background: #00f0c0; }
  button.danger { background: #ff4d4d; color: #fff; }
  input, select { background: #0d0d0d; color: #e0e0e0; border: 1px solid #333; border-radius: 4px;
                  padding: 6px 8px; font-size: 13px; width: 100%; margin-bottom: 8px; }
  .row { display: flex; gap: 6px; align-items: center; margin-bottom: 8px; }
  .badge { display: inline-block; padding: 2px 8px; border-radius: 3px; font-size: 11px;
           font-weight: 600; }
  .badge.get { background: #1a3a2a; color: #4ade80; }
  .badge.post { background: #3a2a1a; color: #fbbf24; }
  .badge.put { background: #1a2a3a; color: #60a5fa; }
  .badge.patch { background: #2a1a3a; color: #c084fc; }
  .badge.delete { background: #3a1a1a; color: #f87171; }
  .status { font-size: 12px; margin-bottom: 8px; }
  .connected { color: #4ade80; }
  .disconnected { color: #f87171; }
  .endpoint { font-size: 12px; color: #888; margin: 4px 0; }
  .endpoint code { color: #00d4aa; background: #1a1a1a; padding: 1px 6px; border-radius: 3px; }
</style>
</head>
<body>
<h1>localup Test Server</h1>
<p class="subtitle">WebSocket + SSE + All HTTP Methods</p>

<div class="grid">
  <!-- HTTP Methods -->
  <div class="card">
    <h2>HTTP Methods</h2>
    <div class="endpoint">
      <span class="badge get">GET</span> <code>/api/echo</code>
      <span class="badge post">POST</span> <code>/api/echo</code>
      <span class="badge put">PUT</span> <code>/api/echo</code>
      <span class="badge patch">PATCH</span> <code>/api/echo</code>
      <span class="badge delete">DELETE</span> <code>/api/echo</code>
    </div>
    <div class="row" style="margin-top: 12px;">
      <select id="method">
        <option>GET</option><option>POST</option><option>PUT</option>
        <option>PATCH</option><option>DELETE</option>
      </select>
    </div>
    <input id="httpBody" placeholder='{"message": "hello"}' value='{"message": "hello from localup"}'>
    <div class="row">
      <button onclick="sendHttp()">Send Request</button>
    </div>
    <div id="httpLog" class="log"></div>
  </div>

  <!-- WebSocket -->
  <div class="card">
    <h2>WebSocket</h2>
    <div class="endpoint"><code>ws://HOST/ws</code></div>
    <div id="wsStatus" class="status disconnected">Disconnected</div>
    <div class="row">
      <button onclick="wsConnect()">Connect</button>
      <button class="danger" onclick="wsDisconnect()">Disconnect</button>
    </div>
    <input id="wsMsg" placeholder="Type a message..." value="Hello WebSocket!">
    <button onclick="wsSend()">Send</button>
    <div id="wsLog" class="log"></div>
  </div>

  <!-- SSE -->
  <div class="card">
    <h2>Server-Sent Events</h2>
    <div class="endpoint"><code>/sse</code></div>
    <div id="sseStatus" class="status disconnected">Disconnected</div>
    <div class="row">
      <button onclick="sseConnect()">Connect</button>
      <button class="danger" onclick="sseDisconnect()">Disconnect</button>
    </div>
    <div id="sseLog" class="log"></div>
  </div>

  <!-- Endpoints Reference -->
  <div class="card">
    <h2>Endpoints</h2>
    <div class="endpoint"><span class="badge get">GET</span> <code>/</code> This dashboard</div>
    <div class="endpoint"><span class="badge get">GET</span> <code>/health</code> Health check</div>
    <div class="endpoint"><span class="badge get">GET</span> <code>/api/echo</code> Echo with request info</div>
    <div class="endpoint"><span class="badge post">POST</span> <code>/api/echo</code> Echo with body</div>
    <div class="endpoint"><span class="badge put">PUT</span> <code>/api/echo</code> Echo PUT</div>
    <div class="endpoint"><span class="badge patch">PATCH</span> <code>/api/echo</code> Echo PATCH</div>
    <div class="endpoint"><span class="badge delete">DELETE</span> <code>/api/echo</code> Echo DELETE</div>
    <div class="endpoint"><span class="badge get">GET</span> <code>/api/large?size=N</code> Large response (N bytes)</div>
    <div class="endpoint"><span class="badge get">GET</span> <code>/api/slow?delay=N</code> Slow response (N sec)</div>
    <div class="endpoint"><span class="badge get">GET</span> <code>/api/status/{code}</code> Custom status code</div>
    <div class="endpoint"><span class="badge get">GET</span> <code>/api/headers</code> Echo request headers</div>
    <div class="endpoint"><span class="badge post">POST</span> <code>/api/upload</code> File upload</div>
    <div class="endpoint"><span class="badge get">GET</span> <code>/sse</code> SSE stream</div>
    <div class="endpoint" style="color:#00d4aa;">WS <code>/ws</code> WebSocket echo + broadcast</div>
  </div>
</div>

<script>
const log = (id, msg) => {
  const el = document.getElementById(id);
  const ts = new Date().toLocaleTimeString();
  el.textContent += `[${ts}] ${msg}\\n`;
  el.scrollTop = el.scrollHeight;
};

// --- HTTP ---
async function sendHttp() {
  const method = document.getElementById('method').value;
  const body = document.getElementById('httpBody').value;
  const opts = { method, headers: { 'Content-Type': 'application/json' } };
  if (method !== 'GET' && method !== 'HEAD') opts.body = body;
  log('httpLog', `>>> ${method} /api/echo`);
  try {
    const res = await fetch('/api/echo', opts);
    const data = await res.json();
    log('httpLog', `<<< ${res.status} ${JSON.stringify(data, null, 2)}`);
  } catch (e) { log('httpLog', `ERR: ${e.message}`); }
}

// --- WebSocket ---
let ws = null;
function wsConnect() {
  if (ws) ws.close();
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  ws = new WebSocket(`${proto}//${location.host}/ws`);
  ws.onopen = () => { document.getElementById('wsStatus').className = 'status connected';
    document.getElementById('wsStatus').textContent = 'Connected'; log('wsLog', 'Connected'); };
  ws.onclose = (e) => { document.getElementById('wsStatus').className = 'status disconnected';
    document.getElementById('wsStatus').textContent = 'Disconnected'; log('wsLog', `Closed: ${e.code}`); ws = null; };
  ws.onmessage = (e) => { log('wsLog', `<<< ${e.data}`); };
  ws.onerror = () => { log('wsLog', 'Error'); };
}
function wsDisconnect() { if (ws) ws.close(); }
function wsSend() {
  if (!ws || ws.readyState !== 1) { log('wsLog', 'Not connected'); return; }
  const msg = document.getElementById('wsMsg').value;
  ws.send(msg); log('wsLog', `>>> ${msg}`);
}

// --- SSE ---
let sse = null;
function sseConnect() {
  if (sse) sse.close();
  sse = new EventSource('/sse');
  sse.onopen = () => { document.getElementById('sseStatus').className = 'status connected';
    document.getElementById('sseStatus').textContent = 'Connected'; log('sseLog', 'Connected'); };
  sse.onmessage = (e) => { log('sseLog', `<<< ${e.data}`); };
  sse.addEventListener('tick', (e) => { log('sseLog', `<<< [tick] ${e.data}`); });
  sse.addEventListener('info', (e) => { log('sseLog', `<<< [info] ${e.data}`); });
  sse.onerror = () => { document.getElementById('sseStatus').className = 'status disconnected';
    document.getElementById('sseStatus').textContent = 'Reconnecting...'; log('sseLog', 'Connection lost, retrying...'); };
}
function sseDisconnect() { if (sse) { sse.close(); sse = null;
  document.getElementById('sseStatus').className = 'status disconnected';
  document.getElementById('sseStatus').textContent = 'Disconnected'; log('sseLog', 'Disconnected'); } }
</script>
</body>
</html>
"""


# ---------------------------------------------------------------------------
# HTTP request handler
# ---------------------------------------------------------------------------
class TestHandler(BaseHTTPRequestHandler):
    """Handles HTTP, SSE, and WebSocket upgrade requests."""

    server_version = "localup-test/1.0"

    # Suppress default logging — we do our own
    def log_message(self, format, *args):
        method = args[0].split()[0] if args else "?"
        path = args[0].split()[1] if args and len(args[0].split()) > 1 else "?"
        status = args[1] if len(args) > 1 else "?"
        print(f"[HTTP] {method} {path} -> {status}")

    # ---------- routing ----------

    def do_GET(self):
        path = urlparse(self.path).path
        qs = parse_qs(urlparse(self.path).query)

        # WebSocket upgrade
        if path == "/ws":
            self._handle_ws_upgrade()
            return

        # SSE
        if path == "/sse":
            self._handle_sse()
            return

        # Dashboard
        if path == "/":
            self._send_html(DASHBOARD_HTML)
            return

        # Health
        if path == "/health":
            self._send_json(200, {"status": "ok", "timestamp": self._now()})
            return

        # Echo
        if path == "/api/echo":
            self._handle_echo("GET")
            return

        # Headers
        if path == "/api/headers":
            headers = {k: v for k, v in self.headers.items()}
            self._send_json(200, {"headers": headers})
            return

        # Large response
        if path == "/api/large":
            size = int(qs.get("size", [1024])[0])
            size = min(size, 10 * 1024 * 1024)  # Cap at 10MB
            self._send_response_headers(200, "application/octet-stream", size)
            # Send in chunks
            chunk_size = 8192
            sent = 0
            pattern = b"X" * chunk_size
            while sent < size:
                to_send = min(chunk_size, size - sent)
                self.wfile.write(pattern[:to_send])
                sent += to_send
            return

        # Slow response
        if path == "/api/slow":
            delay = float(qs.get("delay", [2])[0])
            delay = min(delay, 30)  # Cap at 30s
            time.sleep(delay)
            self._send_json(
                200,
                {
                    "message": f"Responded after {delay}s delay",
                    "delay": delay,
                    "timestamp": self._now(),
                },
            )
            return

        # Custom status code: /api/status/404, /api/status/503, etc.
        if path.startswith("/api/status/"):
            try:
                code = int(path.split("/")[-1])
            except ValueError:
                code = 400
            self._send_json(
                code,
                {
                    "status": code,
                    "message": f"Custom status {code}",
                    "timestamp": self._now(),
                },
            )
            return

        # 404
        self._send_json(404, {"error": "Not found", "path": path})

    def do_POST(self):
        path = urlparse(self.path).path
        if path == "/api/echo":
            self._handle_echo("POST")
        elif path == "/api/upload":
            self._handle_upload()
        else:
            self._send_json(404, {"error": "Not found", "path": path})

    def do_PUT(self):
        path = urlparse(self.path).path
        if path == "/api/echo":
            self._handle_echo("PUT")
        else:
            self._send_json(404, {"error": "Not found", "path": path})

    def do_PATCH(self):
        path = urlparse(self.path).path
        if path == "/api/echo":
            self._handle_echo("PATCH")
        else:
            self._send_json(404, {"error": "Not found", "path": path})

    def do_DELETE(self):
        path = urlparse(self.path).path
        if path == "/api/echo":
            self._handle_echo("DELETE")
        else:
            self._send_json(404, {"error": "Not found", "path": path})

    def do_HEAD(self):
        path = urlparse(self.path).path
        if path == "/health":
            self._send_response_headers(200, "application/json")
        else:
            self._send_response_headers(200, "text/plain")

    def do_OPTIONS(self):
        self.send_response(204)
        self._cors_headers()
        self.send_header("Allow", "GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS")
        self.end_headers()

    # ---------- echo handler ----------

    def _handle_echo(self, method: str):
        body = self._read_body()
        body_str = body.decode("utf-8", errors="replace") if body else ""
        body_json = None
        if body:
            try:
                body_json = json.loads(body_str)
            except (json.JSONDecodeError, ValueError):
                pass

        qs = parse_qs(urlparse(self.path).query)
        headers = {k: v for k, v in self.headers.items()}

        response = {
            "method": method,
            "path": urlparse(self.path).path,
            "query": {k: v[0] if len(v) == 1 else v for k, v in qs.items()},
            "headers": headers,
            "body": body_json if body_json is not None else body_str,
            "body_size": len(body) if body else 0,
            "timestamp": self._now(),
            "server": "localup-test/1.0",
        }
        self._send_json(200, response)

    # ---------- upload handler ----------

    def _handle_upload(self):
        content_type = self.headers.get("Content-Type", "")
        body = self._read_body()

        if "multipart/form-data" in content_type:
            # Simple multipart parsing
            boundary = content_type.split("boundary=")[-1].strip()
            parts = body.split(f"--{boundary}".encode())
            files = []
            for part in parts:
                if b"filename=" in part:
                    # Extract filename
                    header_end = part.find(b"\r\n\r\n")
                    if header_end == -1:
                        continue
                    header_section = part[:header_end].decode("utf-8", errors="replace")
                    file_data = part[header_end + 4 :]
                    if file_data.endswith(b"\r\n"):
                        file_data = file_data[:-2]
                    # Extract filename from Content-Disposition
                    fname = "unknown"
                    for line in header_section.split("\r\n"):
                        if "filename=" in line:
                            fname = line.split('filename="')[-1].rstrip('"')
                            break
                    files.append(
                        {
                            "filename": fname,
                            "size": len(file_data),
                            "md5": hashlib.md5(file_data).hexdigest(),
                        }
                    )
            self._send_json(
                200,
                {
                    "message": "Upload received",
                    "files": files,
                    "timestamp": self._now(),
                },
            )
        else:
            self._send_json(
                200,
                {
                    "message": "Upload received",
                    "content_type": content_type,
                    "size": len(body),
                    "timestamp": self._now(),
                },
            )

    # ---------- SSE handler ----------

    def _handle_sse(self):
        global sse_counter
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "keep-alive")
        self.send_header("X-Accel-Buffering", "no")
        self._cors_headers()
        self.end_headers()

        # Send initial info event
        info = json.dumps(
            {
                "message": "Connected to SSE stream",
                "timestamp": self._now(),
            }
        )
        self.wfile.write(f"event: info\ndata: {info}\n\n".encode())
        self.wfile.flush()

        try:
            while True:
                with sse_counter_lock:
                    sse_counter += 1
                    count = sse_counter

                data = json.dumps(
                    {
                        "count": count,
                        "timestamp": self._now(),
                        "ws_clients": len(ws_clients),
                    }
                )
                # Send as named event "tick" and also as generic message
                self.wfile.write(f"id: {count}\nevent: tick\ndata: {data}\n\n".encode())
                self.wfile.flush()
                time.sleep(1)
        except (BrokenPipeError, ConnectionResetError, OSError):
            pass

    # ---------- WebSocket upgrade ----------

    def _handle_ws_upgrade(self):
        key = self.headers.get("Sec-WebSocket-Key")
        if not key:
            self._send_json(400, {"error": "Missing Sec-WebSocket-Key header"})
            return

        # Send 101 Switching Protocols
        accept = ws_accept_key(key)
        response = (
            "HTTP/1.1 101 Switching Protocols\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Accept: {accept}\r\n"
            "\r\n"
        )
        self.wfile.write(response.encode())
        self.wfile.flush()

        # Hand off to WebSocket handler in a thread
        # We need the raw socket — steal it from the handler
        conn = self.request
        addr = self.client_address

        # Prevent BaseHTTPRequestHandler from closing the socket
        self.close_connection = True

        # Handle in a new thread
        t = threading.Thread(target=ws_handle_client, args=(conn, addr), daemon=True)
        t.start()

    # ---------- helpers ----------

    def _read_body(self) -> bytes:
        length = int(self.headers.get("Content-Length", 0))
        if length > 0:
            return self.rfile.read(length)
        return b""

    def _now(self) -> str:
        return datetime.now(timezone.utc).isoformat()

    def _cors_headers(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header(
            "Access-Control-Allow-Methods", "GET, POST, PUT, PATCH, DELETE, OPTIONS"
        )
        self.send_header("Access-Control-Allow-Headers", "Content-Type, Authorization")

    def _send_json(self, code: int, data: dict):
        body = json.dumps(data, indent=2).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self._cors_headers()
        self.end_headers()
        self.wfile.write(body)

    def _send_html(self, html: str):
        body = html.encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self._cors_headers()
        self.end_headers()
        self.wfile.write(body)

    def _send_response_headers(
        self, code: int, content_type: str, content_length: int = 0
    ):
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        if content_length:
            self.send_header("Content-Length", str(content_length))
        self._cors_headers()
        self.end_headers()


# ---------------------------------------------------------------------------
# Threaded HTTP server (handles concurrent connections)
# ---------------------------------------------------------------------------
class ThreadedHTTPServer(HTTPServer):
    """Handle each request in a separate thread."""

    allow_reuse_address = True
    daemon_threads = True

    def process_request(self, request, client_address):
        t = threading.Thread(
            target=self.process_request_thread, args=(request, client_address)
        )
        t.daemon = True
        t.start()

    def process_request_thread(self, request, client_address):
        try:
            self.finish_request(request, client_address)
        except Exception:
            self.handle_error(request, client_address)
        # Don't close the request here — WebSocket connections need to stay open
        # The handler or WS thread will close it when done
        # For normal HTTP requests, the handler already closes via shutdown_request


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main():
    parser = argparse.ArgumentParser(description="localup comprehensive test server")
    parser.add_argument(
        "--port", type=int, default=8080, help="Port to listen on (default: 8080)"
    )
    parser.add_argument(
        "--host", type=str, default="0.0.0.0", help="Host to bind to (default: 0.0.0.0)"
    )
    args = parser.parse_args()

    server = ThreadedHTTPServer((args.host, args.port), TestHandler)

    print(f"")
    print(f"  localup Test Server")
    print(f"  {'=' * 40}")
    print(f"  Listening on http://{args.host}:{args.port}")
    print(f"")
    print(f"  Endpoints:")
    print(f"    GET  /             Dashboard (HTML)")
    print(f"    GET  /health       Health check")
    print(f"    *    /api/echo     Echo (GET/POST/PUT/PATCH/DELETE)")
    print(f"    GET  /api/headers  Echo request headers")
    print(f"    GET  /api/large    Large response (?size=N)")
    print(f"    GET  /api/slow     Slow response (?delay=N)")
    print(f"    GET  /api/status/N Custom status code")
    print(f"    POST /api/upload   File upload")
    print(f"    GET  /sse          Server-Sent Events stream")
    print(f"    WS   /ws           WebSocket echo + broadcast")
    print(f"")
    print(f"  Press Ctrl+C to stop")
    print(f"")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...")
        server.shutdown()


if __name__ == "__main__":
    main()
