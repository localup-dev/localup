// Example: TCP tunnel using the LocalUp Go SDK
//
// This example demonstrates how to expose a local TCP service to the internet
// using LocalUp. It includes a built-in echo server for easy testing.
//
// Usage:
//
//	go run main.go
//
// Environment variables:
//
//	LOCALUP_AUTHTOKEN - Your LocalUp authentication token
//	LOCALUP_RELAY     - Relay server address (default: localhost:5443)
//	LOCAL_PORT        - Local TCP port to expose (default: starts echo server)
//	LOCALUP_LOG       - Log level: debug, info, warn, error, none (default: info)
//
// Testing with netcat:
//
//	nc <relay-host> <assigned-port>
//	Hello, world!    <- type this
//	Hello, world!    <- echo server responds
package main

import (
	"context"
	"fmt"
	"io"
	"log"
	"net"
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
		relayAddr = "localhost:5443"
	}

	// Check if we should use a custom port or start the built-in echo server
	var localPort uint16
	useEchoServer := true

	if portStr := os.Getenv("LOCAL_PORT"); portStr != "" {
		p, err := strconv.ParseUint(portStr, 10, 16)
		if err != nil {
			return fmt.Errorf("invalid LOCAL_PORT: %w", err)
		}
		localPort = uint16(p)
		useEchoServer = false
	}

	// Start the echo server if no LOCAL_PORT specified
	if useEchoServer {
		listener, err := net.Listen("tcp", "127.0.0.1:0")
		if err != nil {
			return fmt.Errorf("failed to start echo server: %w", err)
		}
		defer listener.Close()

		// Get the assigned port
		localPort = uint16(listener.Addr().(*net.TCPAddr).Port)
		fmt.Printf("Started echo server on localhost:%d\n", localPort)

		// Run echo server in background
		go runEchoServer(ctx, listener)
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

	// Create TCP tunnel
	// Port 0 means auto-assign a public port
	ln, err := agent.Forward(ctx,
		localup.WithUpstream(fmt.Sprintf("localhost:%d", localPort)),
		localup.WithProtocol(localup.ProtocolTCP),
		localup.WithPort(0), // auto-assign public port
	)
	if err != nil {
		return fmt.Errorf("failed to create tunnel: %w", err)
	}

	fmt.Println()
	fmt.Println("TCP Tunnel online!")
	fmt.Printf("Forwarding from %s to localhost:%d\n", ln.URL(), localPort)
	fmt.Println()
	if useEchoServer {
		fmt.Println("Testing with netcat:")
		fmt.Println("  nc <relay-host> <assigned-port>")
		fmt.Println("  Type something and press Enter - the echo server will respond!")
	} else {
		fmt.Println("Example usage:")
		fmt.Printf("  nc <relay-host> <assigned-port>\n")
	}
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

// runEchoServer runs a simple TCP echo server
func runEchoServer(ctx context.Context, listener net.Listener) {
	for {
		select {
		case <-ctx.Done():
			return
		default:
		}

		conn, err := listener.Accept()
		if err != nil {
			select {
			case <-ctx.Done():
				return
			default:
				log.Printf("Echo server accept error: %v", err)
				continue
			}
		}

		go handleEchoConnection(conn)
	}
}

// handleEchoConnection handles a single echo connection
func handleEchoConnection(conn net.Conn) {
	defer conn.Close()

	log.Printf("Echo server: new connection from %s", conn.RemoteAddr())

	// Send welcome message
	conn.Write([]byte("Welcome to the LocalUp Echo Server!\n"))
	conn.Write([]byte("Type something and press Enter:\n"))

	// Echo everything back
	buf := make([]byte, 4096)
	for {
		n, err := conn.Read(buf)
		if err != nil {
			if err != io.EOF {
				log.Printf("Echo server read error: %v", err)
			}
			return
		}

		log.Printf("Echo server: received %d bytes from %s", n, conn.RemoteAddr())

		// Echo back with prefix
		response := fmt.Sprintf("[echo] %s", string(buf[:n]))
		if _, err := conn.Write([]byte(response)); err != nil {
			log.Printf("Echo server write error: %v", err)
			return
		}
	}
}
