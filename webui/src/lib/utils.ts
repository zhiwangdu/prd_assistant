import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatDuration(nanoseconds?: number | null) {
  if (nanoseconds == null) return "-";
  if (nanoseconds === 0) return "infinite";
  const units = [
    ["d", 86_400_000_000_000],
    ["h", 3_600_000_000_000],
    ["m", 60_000_000_000],
    ["s", 1_000_000_000]
  ] as const;
  for (const [label, size] of units) {
    if (nanoseconds >= size) return `${(nanoseconds / size).toLocaleString(undefined, { maximumFractionDigits: 2 })}${label}`;
  }
  return `${nanoseconds.toLocaleString()}ns`;
}

export function valueOrDash(value: unknown) {
  return value === null || value === undefined || value === "" ? "-" : String(value);
}
