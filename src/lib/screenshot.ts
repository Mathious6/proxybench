import type { SubnetRow } from "./import";

const probedAt = Date.UTC(2026, 7, 24, 14, 30);

const countries = ["US", "GB", "DE", "CA", "NL", "FR", "JP", "SG", "AU", null];
const tagSets = [
  ["residential"],
  ["mobile"],
  ["checkout"],
  [],
  ["backup"],
  ["monitoring"],
  ["europe"],
  ["retail"],
  ["stable"],
  [],
];

export const screenshotRows: SubnetRow[] = Array.from({ length: 36 }, (_, index) => {
  const state = index % 6;
  const measured = state !== 5;
  const hasMetrics = measured && state !== 4;
  const quantity = 24 + ((index * 13) % 77);
  const ok = state === 4 ? 0 : measured ? quantity - ((index * 3) % 9) : null;
  const base = state === 0 ? 180 : state === 1 ? 720 : state === 2 ? 1840 : 420;

  return {
    cidr: `198.18.${index}.0/24`,
    country: countries[index % countries.length],
    quantity,
    tags: tagSets[index % tagSets.length],
    ok,
    connectP50: hasMetrics ? base : null,
    connectP95: hasMetrics ? base + 90 : null,
    ttfbP50: hasMetrics ? base + 140 : null,
    ttfbP95: hasMetrics ? base + 330 : null,
    lastRunAt: measured ? probedAt : null,
  };
});

export const screenshotSelectedCidrs = new Set(
  screenshotRows.slice(0, 3).map((row) => row.cidr),
);
