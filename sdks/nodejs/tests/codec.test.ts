/**
 * Tests for bincode codec
 */

import { describe, expect, test } from "bun:test";
import {
  encodeMessage,
  decodeMessage,
  encodeMessagePayload,
  decodeMessagePayload,
  FrameAccumulator,
} from "../src/protocol/codec.ts";
import type { TunnelMessage } from "../src/protocol/types.ts";

describe("bincode codec", () => {
  test("encode/decode Ping message", () => {
    const msg: TunnelMessage = { type: "Ping", timestamp: 12345n };
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded);

    expect(decoded.type).toBe("Ping");
    expect((decoded as Extract<TunnelMessage, { type: "Ping" }>).timestamp).toBe(12345n);
  });

  test("encode/decode Pong message", () => {
    const msg: TunnelMessage = { type: "Pong", timestamp: 67890n };
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded);

    expect(decoded.type).toBe("Pong");
    expect((decoded as Extract<TunnelMessage, { type: "Pong" }>).timestamp).toBe(67890n);
  });

  test("encode/decode Connect message", () => {
    const msg: TunnelMessage = {
      type: "Connect",
      localupId: "test-tunnel-123",
      authToken: "jwt-token-here",
      protocols: [{ type: "Http", subdomain: "myapp" }],
      config: {
        localHost: "localhost",
        localPort: 8080,
        localHttps: false,
        exitNode: { type: "Custom", address: "relay.example.com:4443" },
        failover: true,
        ipAllowlist: ["10.0.0.0/8"],
        enableCompression: false,
        enableMultiplexing: true,
      },
    };

    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "Connect" }>;

    expect(decoded.type).toBe("Connect");
    expect(decoded.localupId).toBe("test-tunnel-123");
    expect(decoded.authToken).toBe("jwt-token-here");
    expect(decoded.protocols).toHaveLength(1);
    expect(decoded.protocols[0]?.type).toBe("Http");
    expect((decoded.protocols[0] as { type: "Http"; subdomain: string | null }).subdomain).toBe(
      "myapp"
    );
    expect(decoded.config.localPort).toBe(8080);
    expect(decoded.config.ipAllowlist).toContain("10.0.0.0/8");
  });

  test("encode/decode Connected message", () => {
    const msg: TunnelMessage = {
      type: "Connected",
      localupId: "test-tunnel-123",
      endpoints: [
        {
          protocol: { type: "Http", subdomain: "myapp" },
          publicUrl: "https://myapp.relay.example.com",
          port: null,
        },
      ],
    };

    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "Connected" }>;

    expect(decoded.type).toBe("Connected");
    expect(decoded.endpoints[0]?.publicUrl).toBe("https://myapp.relay.example.com");
  });

  test("encode/decode Disconnect message", () => {
    const msg: TunnelMessage = { type: "Disconnect", reason: "Client closed" };
    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "Disconnect" }>;

    expect(decoded.type).toBe("Disconnect");
    expect(decoded.reason).toBe("Client closed");
  });

  test("encode/decode TcpData message with binary data", () => {
    const data = new Uint8Array([0, 1, 2, 3, 255, 254, 253]);
    const msg: TunnelMessage = {
      type: "TcpData",
      streamId: 42,
      data,
    };

    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "TcpData" }>;

    expect(decoded.type).toBe("TcpData");
    expect(decoded.streamId).toBe(42);
    expect(decoded.data).toEqual(data);
  });

  test("encode/decode HttpRequest message", () => {
    const body = new TextEncoder().encode('{"key": "value"}');
    const msg: TunnelMessage = {
      type: "HttpRequest",
      streamId: 1,
      method: "POST",
      uri: "/api/test",
      headers: [
        ["content-type", "application/json"],
        ["x-custom", "header"],
      ],
      body,
    };

    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "HttpRequest" }>;

    expect(decoded.type).toBe("HttpRequest");
    expect(decoded.method).toBe("POST");
    expect(decoded.uri).toBe("/api/test");
    expect(decoded.headers).toHaveLength(2);
    expect(decoded.body).toEqual(body);
  });

  test("encode/decode HttpRequest with null body", () => {
    const msg: TunnelMessage = {
      type: "HttpRequest",
      streamId: 1,
      method: "GET",
      uri: "/",
      headers: [],
      body: null,
    };

    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "HttpRequest" }>;

    expect(decoded.type).toBe("HttpRequest");
    expect(decoded.body).toBeNull();
  });

  test("payload encode/decode without length header", () => {
    const msg: TunnelMessage = { type: "Ping", timestamp: 999n };
    const payload = encodeMessagePayload(msg);
    const decoded = decodeMessagePayload(payload);

    expect(decoded.type).toBe("Ping");
    expect((decoded as Extract<TunnelMessage, { type: "Ping" }>).timestamp).toBe(999n);
  });
});

describe("FrameAccumulator", () => {
  test("accumulate and extract single message", () => {
    const msg: TunnelMessage = { type: "Ping", timestamp: 123n };
    const encoded = encodeMessage(msg);

    const acc = new FrameAccumulator();
    acc.push(encoded);

    const decoded = acc.tryReadMessage();
    expect(decoded).not.toBeNull();
    expect(decoded?.type).toBe("Ping");
  });

  test("accumulate partial data then complete", () => {
    const msg: TunnelMessage = { type: "Pong", timestamp: 456n };
    const encoded = encodeMessage(msg);

    const acc = new FrameAccumulator();

    // Push first half
    acc.push(encoded.slice(0, 5));
    expect(acc.tryReadMessage()).toBeNull();

    // Push second half
    acc.push(encoded.slice(5));
    const decoded = acc.tryReadMessage();

    expect(decoded).not.toBeNull();
    expect(decoded?.type).toBe("Pong");
  });

  test("accumulate multiple messages", () => {
    const msg1: TunnelMessage = { type: "Ping", timestamp: 1n };
    const msg2: TunnelMessage = { type: "Pong", timestamp: 2n };

    const acc = new FrameAccumulator();
    acc.push(encodeMessage(msg1));
    acc.push(encodeMessage(msg2));

    const messages = acc.readAllMessages();
    expect(messages).toHaveLength(2);
    expect(messages[0]?.type).toBe("Ping");
    expect(messages[1]?.type).toBe("Pong");
  });

  test("clear accumulator", () => {
    const msg: TunnelMessage = { type: "Ping", timestamp: 1n };
    const acc = new FrameAccumulator();
    acc.push(encodeMessage(msg));

    expect(acc.size()).toBeGreaterThan(0);
    acc.clear();
    expect(acc.size()).toBe(0);
  });
});

describe("protocol types", () => {
  test("encode/decode Tcp protocol", () => {
    const msg: TunnelMessage = {
      type: "Connect",
      localupId: "test",
      authToken: "token",
      protocols: [{ type: "Tcp", port: 5432 }],
      config: {
        localHost: "localhost",
        localPort: null,
        localHttps: false,
        exitNode: { type: "Auto" },
        failover: true,
        ipAllowlist: [],
        enableCompression: false,
        enableMultiplexing: true,
      },
    };

    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "Connect" }>;

    expect(decoded.protocols[0]?.type).toBe("Tcp");
    expect((decoded.protocols[0] as { type: "Tcp"; port: number }).port).toBe(5432);
  });

  test("encode/decode Tls protocol", () => {
    const msg: TunnelMessage = {
      type: "Connect",
      localupId: "test",
      authToken: "token",
      protocols: [{ type: "Tls", port: 443, sniPattern: "*.example.com" }],
      config: {
        localHost: "localhost",
        localPort: null,
        localHttps: false,
        exitNode: { type: "Nearest" },
        failover: true,
        ipAllowlist: [],
        enableCompression: false,
        enableMultiplexing: true,
      },
    };

    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "Connect" }>;

    expect(decoded.protocols[0]?.type).toBe("Tls");
    const proto = decoded.protocols[0] as { type: "Tls"; port: number; sniPattern: string };
    expect(proto.port).toBe(443);
    expect(proto.sniPattern).toBe("*.example.com");
  });

  test("encode/decode Https protocol with null subdomain", () => {
    const msg: TunnelMessage = {
      type: "Connect",
      localupId: "test",
      authToken: "token",
      protocols: [{ type: "Https", subdomain: null }],
      config: {
        localHost: "localhost",
        localPort: null,
        localHttps: false,
        exitNode: { type: "Specific", region: "UsEast" },
        failover: true,
        ipAllowlist: [],
        enableCompression: false,
        enableMultiplexing: true,
      },
    };

    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "Connect" }>;

    expect(decoded.protocols[0]?.type).toBe("Https");
    expect((decoded.protocols[0] as { type: "Https"; subdomain: string | null }).subdomain).toBeNull();
    expect(decoded.config.exitNode.type).toBe("Specific");
  });

  test("encode/decode MultiRegion exit node", () => {
    const msg: TunnelMessage = {
      type: "Connect",
      localupId: "test",
      authToken: "token",
      protocols: [{ type: "Http", subdomain: "test" }],
      config: {
        localHost: "localhost",
        localPort: null,
        localHttps: false,
        exitNode: { type: "MultiRegion", regions: ["UsEast", "EuWest", "AsiaPacific"] },
        failover: true,
        ipAllowlist: [],
        enableCompression: false,
        enableMultiplexing: true,
      },
    };

    const encoded = encodeMessage(msg);
    const decoded = decodeMessage(encoded) as Extract<TunnelMessage, { type: "Connect" }>;

    expect(decoded.config.exitNode.type).toBe("MultiRegion");
    const exitNode = decoded.config.exitNode as { type: "MultiRegion"; regions: string[] };
    expect(exitNode.regions).toHaveLength(3);
    expect(exitNode.regions).toContain("UsEast");
    expect(exitNode.regions).toContain("EuWest");
    expect(exitNode.regions).toContain("AsiaPacific");
  });
});
