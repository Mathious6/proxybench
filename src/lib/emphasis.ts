export type Tone = "good" | "bad" | "muted";

const FAST_MS = 500;
const SLOW_MS = 1500;
const OK_HIGH = 0.8;
const OK_LOW = 0.3;

export type TimingKey = "connectP50" | "connectP95" | "ttfbP50" | "ttfbP95";

export function timingTone(value: number | null): Tone {
  if (value === null) {
    return "muted";
  }
  if (value < FAST_MS) {
    return "good";
  }
  if (value > SLOW_MS) {
    return "bad";
  }
  return "muted";
}

export function okTone(ok: number | null, quantity: number): Tone {
  if (ok === null || quantity <= 0) {
    return "muted";
  }
  const ratio = ok / quantity;
  if (ratio >= OK_HIGH) {
    return "good";
  }
  if (ratio < OK_LOW) {
    return "bad";
  }
  return "muted";
}

export function toneClass(tone: Tone): string {
  if (tone === "good") {
    return "text-good";
  }
  if (tone === "bad") {
    return "text-bad";
  }
  return "text-muted";
}
