export type Tone = "good" | "bad" | "plain" | "muted" | "faint";

const FAST_MS = 500;
const SLOW_MS = 1500;
const OK_HIGH = 0.8;
const OK_LOW = 0.3;

const whole = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });
const tenth = new Intl.NumberFormat("en-US", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
const fullDate = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

export const DASH = "—";

export function count(value: number): string {
  return whole.format(value);
}

export function plural(value: number, one: string, many: string): string {
  return `${count(value)} ${value === 1 ? one : many}`;
}

export function timing(ms: number | null): string {
  if (ms === null) {
    return DASH;
  }
  if (ms < 1000) {
    return `${whole.format(Math.round(ms))} ms`;
  }
  const seconds = ms / 1000;
  return seconds >= 10 ? `${whole.format(Math.round(seconds))} s` : `${tenth.format(seconds)} s`;
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

export function stamp(at: number | null): string {
  if (at === null) {
    return DASH;
  }
  const date = new Date(at);
  return `${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function stampTitle(at: number | null): string | undefined {
  return at === null ? undefined : fullDate.format(at);
}

export function eta(seconds: number | null): string {
  if (seconds === null) {
    return "";
  }
  return seconds < 60 ? `~${Math.max(1, seconds)}s` : `~${Math.round(seconds / 60)}m`;
}

export function p50Tone(ms: number | null): Tone {
  if (ms === null) {
    return "faint";
  }
  if (ms < FAST_MS) {
    return "good";
  }
  return ms > SLOW_MS ? "bad" : "plain";
}

export function p95Tone(ms: number | null): Tone {
  if (ms === null) {
    return "faint";
  }
  return ms > SLOW_MS ? "bad" : "muted";
}

export function okTone(ok: number, tested: number): Tone {
  if (tested <= 0) {
    return "plain";
  }
  const ratio = ok / tested;
  if (ratio >= OK_HIGH) {
    return "good";
  }
  return ratio < OK_LOW ? "bad" : "plain";
}
