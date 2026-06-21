import { logToBackend, type LogLevel } from "./tauri";

// ponytail: spec FR-G1 + audit §4.2. Single class, no framework. The
// rate limiter is a 1-second counter window — if more than
// MAX_FORWARD_PER_SECOND entries fire inside the window, drop until it
// resets. Prevents a tight render-loop error from spamming the file
// log. Best-effort: a backend failure never throws back to callers.

const FORWARD_LEVELS: ReadonlySet<LogLevel> = new Set(["warn", "error"]);
const MAX_FORWARD_PER_SECOND = 10;

const isDev = import.meta.env.DEV;
const DEV_FORWARD: ReadonlySet<LogLevel> = new Set(["warn", "error"]);

type Fields = Record<string, unknown>;

export class Logger {
  private windowStart = 0;
  private windowCount = 0;

  private shouldForward(level: LogLevel): boolean {
    if (isDev) return DEV_FORWARD.has(level);
    return FORWARD_LEVELS.has(level) || level === "info";
  }

  private rateLimited(): boolean {
    const now = Date.now();
    if (now - this.windowStart > 1000) {
      this.windowStart = now;
      this.windowCount = 0;
    }
    this.windowCount += 1;
    return this.windowCount > MAX_FORWARD_PER_SECOND;
  }

  private send(level: LogLevel, msg: string, ctx?: Fields): void {
    if (!this.shouldForward(level)) return;
    if (this.rateLimited()) return;
    if (isDev) {
      const line = ctx ? `${msg} ${JSON.stringify(ctx)}` : msg;
      if (level === "error") console.error(line);
      else if (level === "warn") console.warn(line);
      else console.info(line);
    }
    logToBackend(level, msg, ctx).catch(() => {});
  }

  info(msg: string, ctx?: Fields): void {
    this.send("info", msg, ctx);
  }
  warn(msg: string, ctx?: Fields): void {
    this.send("warn", msg, ctx);
  }
  error(msg: string, ctx?: Fields): void {
    this.send("error", msg, ctx);
  }
  debug(msg: string, ctx?: Fields): void {
    if (!isDev) return;
    const line = ctx ? `${msg} ${JSON.stringify(ctx)}` : msg;
    console.info(line);
  }
}

export const log = new Logger();
