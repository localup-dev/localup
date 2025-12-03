// Example: Simple HTTP tunnel using the LocalUp Go SDK
//
// This example demonstrates how to expose a local HTTP server to the internet
// using LocalUp. It's similar to the ngrok Go SDK API.
//
// Usage:
//
//	go run main.go
//
// Environment variables:
//
//	LOCALUP_AUTHTOKEN - Your LocalUp authentication token
//	LOCALUP_RELAY     - Relay server address (default: localhost:4443)
//	LOCALUP_SUBDOMAIN - Optional subdomain to request
//	LOCALUP_LOG       - Log level: debug, info, warn, error, none (default: info)
package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/localup/localup-go"
)

const localAddress = "http://localhost:8080"

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

	subdomain := os.Getenv("LOCALUP_SUBDOMAIN")

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

	// Build tunnel options
	opts := []localup.TunnelOption{
		localup.WithUpstream(localAddress),
		localup.WithProtocol(localup.ProtocolHTTP),
	}

	if subdomain != "" {
		opts = append(opts, localup.WithSubdomain(subdomain))
	}

	// Create the tunnel
	ln, err := agent.Forward(ctx, opts...)
	if err != nil {
		return fmt.Errorf("failed to create tunnel: %w", err)
	}

	fmt.Println("Tunnel online!")
	fmt.Printf("Forwarding from %s to %s\n", ln.URL(), localAddress)
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
