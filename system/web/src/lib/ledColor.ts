// Colour helpers for LED pickers; channels are raw 0-255 strip values.
import type { WakeLightEffect } from "@/lib/api";

export type RGB = [number, number, number];

export interface WakeLook {
  color: RGB;
  effect: WakeLightEffect;
  speed: number;
}

// The board's `listening` look in hal/presets.py; shown until the owner picks another.
export const WAKE_LOOK_DEFAULT: WakeLook = { color: [4, 8, 16], effect: "breathing", speed: 1.2 };

export function toHsv([r, g, b]: RGB): { h: number; s: number; v: number } {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h = (h * 60 + 360) % 360;
  }
  return { h, s: max === 0 ? 0 : d / max, v: max };
}

export function fromHsv(h: number, s: number, v: number): RGB {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  const [r, g, b] =
    h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x]
      : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [Math.round(r + m), Math.round(g + m), Math.round(b + m)];
}

// Screen swatch: the strip values are far too dark to show as-is, so scale to full brightness.
export function swatch(color: RGB): string {
  const max = Math.max(...color);
  if (max === 0) return "transparent";
  const k = 255 / max;
  return `rgb(${color.map((c) => Math.round(c * k)).join(",")})`;
}
