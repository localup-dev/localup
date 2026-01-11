/**
 * Simple logger with configurable log levels
 */

export type LogLevel = "debug" | "info" | "warn" | "error" | "silent";

const LOG_LEVELS: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
  silent: 4,
};

let currentLevel: LogLevel = "info";

/**
 * Set the global log level
 */
export function setLogLevel(level: LogLevel): void {
  currentLevel = level;
}

/**
 * Get the current log level
 */
export function getLogLevel(): LogLevel {
  return currentLevel;
}

function shouldLog(level: LogLevel): boolean {
  return LOG_LEVELS[level] >= LOG_LEVELS[currentLevel];
}

/**
 * Logger interface
 */
export const logger = {
  debug(message: string, ...args: unknown[]): void {
    if (shouldLog("debug")) {
      console.debug(`[DEBUG] ${message}`, ...args);
    }
  },

  info(message: string, ...args: unknown[]): void {
    if (shouldLog("info")) {
      console.info(`[INFO] ${message}`, ...args);
    }
  },

  warn(message: string, ...args: unknown[]): void {
    if (shouldLog("warn")) {
      console.warn(`[WARN] ${message}`, ...args);
    }
  },

  error(message: string, ...args: unknown[]): void {
    if (shouldLog("error")) {
      console.error(`[ERROR] ${message}`, ...args);
    }
  },
};
