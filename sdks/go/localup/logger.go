package localup

import (
	"fmt"
	"log"
	"os"
	"strings"
)

// Logger is the interface for logging in the SDK.
type Logger interface {
	Debug(msg string, keysAndValues ...interface{})
	Info(msg string, keysAndValues ...interface{})
	Warn(msg string, keysAndValues ...interface{})
	Error(msg string, keysAndValues ...interface{})
}

// LogLevelFromEnv returns a LogLevel based on the LOCALUP_LOG environment variable.
// Valid values: "debug", "info", "warn", "error", "none"
// Default: LogLevelInfo
func LogLevelFromEnv() LogLevel {
	level := strings.ToLower(os.Getenv("LOCALUP_LOG"))
	switch level {
	case "debug":
		return LogLevelDebug
	case "info":
		return LogLevelInfo
	case "warn", "warning":
		return LogLevelWarn
	case "error":
		return LogLevelError
	case "none", "off", "disabled":
		return LogLevelNone
	default:
		return LogLevelInfo
	}
}

// LoggerFromEnv creates a logger based on the LOCALUP_LOG environment variable.
// If LOCALUP_LOG is "none", returns a no-op logger.
// Otherwise returns a standard logger at the specified level.
func LoggerFromEnv() Logger {
	level := LogLevelFromEnv()
	if level == LogLevelNone {
		return &noopLogger{}
	}
	return NewStdLogger(level)
}

// noopLogger is a logger that discards all output.
type noopLogger struct{}

func (l *noopLogger) Debug(_ string, _ ...interface{}) {}
func (l *noopLogger) Info(_ string, _ ...interface{})  {}
func (l *noopLogger) Warn(_ string, _ ...interface{})  {}
func (l *noopLogger) Error(_ string, _ ...interface{}) {}

// stdLogger is a simple logger that uses the standard library.
type stdLogger struct {
	logger *log.Logger
	level  LogLevel
}

// LogLevel represents the logging level.
type LogLevel int

const (
	LogLevelDebug LogLevel = iota
	LogLevelInfo
	LogLevelWarn
	LogLevelError
	LogLevelNone // Disables all logging
)

// NewStdLogger creates a new standard library logger.
func NewStdLogger(level LogLevel) Logger {
	return &stdLogger{
		logger: log.New(os.Stderr, "[localup] ", log.LstdFlags),
		level:  level,
	}
}

func (l *stdLogger) Debug(msg string, keysAndValues ...interface{}) {
	if l.level <= LogLevelDebug {
		l.log("DEBUG", msg, keysAndValues...)
	}
}

func (l *stdLogger) Info(msg string, keysAndValues ...interface{}) {
	if l.level <= LogLevelInfo {
		l.log("INFO", msg, keysAndValues...)
	}
}

func (l *stdLogger) Warn(msg string, keysAndValues ...interface{}) {
	if l.level <= LogLevelWarn {
		l.log("WARN", msg, keysAndValues...)
	}
}

func (l *stdLogger) Error(msg string, keysAndValues ...interface{}) {
	if l.level <= LogLevelError {
		l.log("ERROR", msg, keysAndValues...)
	}
}

func (l *stdLogger) log(level, msg string, keysAndValues ...interface{}) {
	if len(keysAndValues) == 0 {
		l.logger.Printf("%s: %s", level, msg)
		return
	}

	// Format key-value pairs
	kvs := ""
	for i := 0; i < len(keysAndValues); i += 2 {
		if i > 0 {
			kvs += " "
		}
		if i+1 < len(keysAndValues) {
			kvs += formatKV(keysAndValues[i], keysAndValues[i+1])
		}
	}
	l.logger.Printf("%s: %s %s", level, msg, kvs)
}

func formatKV(key, value interface{}) string {
	return formatValue(key) + "=" + formatValue(value)
}

func formatValue(v interface{}) string {
	switch val := v.(type) {
	case string:
		return val
	case error:
		return val.Error()
	default:
		return fmt.Sprintf("%v", val)
	}
}
