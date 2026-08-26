export function empty(value: string | number | null | undefined) {
  return value === null || value === undefined || value === "";
}

const grouped = new Intl.NumberFormat(undefined, { useGrouping: true, maximumFractionDigits: 0 });
const groupedTenth = new Intl.NumberFormat(undefined, {
  useGrouping: true,
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
});

export function dash(value: string | number | null | undefined) {
  if (empty(value)) {
    return "—";
  }
  if (typeof value === "number") {
    return grouped.format(value);
  }
  return String(value);
}

export function timing(value: number | null | undefined) {
  if (empty(value)) {
    return "—";
  }
  const ms = Math.round(value as number);
  if (ms >= 1000) {
    const seconds = ms / 1000;
    if (seconds >= 10) {
      return `${grouped.format(Math.round(seconds))} s`;
    }
    return `${groupedTenth.format(seconds)} s`;
  }
  return `${grouped.format(ms)} ms`;
}

const probedAtFull = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

function pad2(value: number) {
  return String(value).padStart(2, "0");
}

export function lastProbe(value: number | null | undefined) {
  if (empty(value)) {
    return "—";
  }
  const date = new Date(value as number);
  return `${pad2(date.getMonth() + 1)}/${pad2(date.getDate())} ${pad2(date.getHours())}:${pad2(date.getMinutes())}`;
}

export function lastProbeTitle(value: number | null | undefined) {
  if (empty(value)) {
    return "";
  }
  return probedAtFull.format(value as number);
}
