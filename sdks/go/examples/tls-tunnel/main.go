// Example: TLS passthrough tunnel using the LocalUp Go SDK
//
// This example demonstrates how to expose a local TLS service to the internet
// using SNI-based routing. The TLS connection is passed through without
// termination - end-to-end encryption is preserved.
//
// Use cases:
//   - Expose a local HTTPS server with your own certificate
//   - Expose a TLS-enabled database (e.g., MySQL with TLS)
//   - Any service where you want to maintain end-to-end TLS
//
// Usage:
//
//	go run main.go
//
// Environment variables:
//
//	LOCALUP_AUTHTOKEN - Your LocalUp authentication token
//	LOCALUP_RELAY     - Relay server address (default: localhost:4443)
//	LOCAL_PORT        - Local TLS port to expose (default: 443)
//	LOCALUP_LOG       - Log level: debug, info, warn, error, none (default: info)
package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/signal"
	"strconv"
	"syscall"

	"github.com/localup/localup-go"
)

func main() {
	if err := run(context.Background()); err != nil {
		log.Fatal(err)
	}
}

func run(ctx context.Context) error {
	// Get configuration from environment
	authtoken := os.Getenv("LOCALUP_AUTHTOKEN")
	if authtoken == "" {
		return fmt.Errorf("LOCALUP_AUTHTOKEN environment variable is required")
	}

	relayAddr := os.Getenv("LOCALUP_RELAY")
	if relayAddr == "" {
		relayAddr = "localhost:4443"
	}

	// Default to HTTPS port
	localPort := uint16(443)
	if portStr := os.Getenv("LOCAL_PORT"); portStr != "" {
		p, err := strconv.ParseUint(portStr, 10, 16)
		if err != nil {
			return fmt.Errorf("invalid LOCAL_PORT: %w", err)
		}
		localPort = uint16(p)
	}

	// Create the agent with logging from LOCALUP_LOG env var
	agent, err := localup.NewAgent(
		localup.WithAuthtoken(authtoken),
		localup.WithRelayAddr(relayAddr),
		localup.WithLogger(localup.LoggerFromEnv()),
	)
	if err != nil {
		return fmt.Errorf("failed to create agent: %w", err)
	}
	defer agent.Close()

	// Create TLS passthrough tunnel
	// Traffic is routed based on SNI (Server Name Indication)
	// The TLS handshake is passed through - relay doesn't terminate TLS
	ln, err := agent.Forward(ctx,
		localup.WithUpstream(fmt.Sprintf("localhost:%d", localPort)),
		localup.WithProtocol(localup.ProtocolTLS),
	)
	if err != nil {
		return fmt.Errorf("failed to create tunnel: %w", err)
	}

	fmt.Println("TLS Passthrough Tunnel online!")
	fmt.Printf("Forwarding from %s to localhost:%d\n", ln.URL(), localPort)
	fmt.Println()
	fmt.Println("Features:")
	fmt.Println("  - End-to-end TLS encryption (relay doesn't see plaintext)")
	fmt.Println("  - SNI-based routing (multiple domains on same port)")
	fmt.Println("  - Your local server handles TLS termination")
	fmt.Println()
	fmt.Println("Press Ctrl+C to stop")

	// Wait for interrupt or tunnel closure
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	select {
	case <-sigCh:
		fmt.Println("\nShutting down...")
	case <-ln.Done():
		fmt.Println("Tunnel closed")
	}

	return nil
}
