/**
 * Colour maths shared by the appearance tests.
 *
 * WCAG contrast answers "can this be read"; CIE L* answers "how strong does
 * this look", which is the right question for a decorative pattern.
 */
export type Rgb = [number, number, number];

export function hexToRgb(hex: string): Rgb {
  const value = hex.replace('#', '');
  return [0, 2, 4].map((offset) => parseInt(value.slice(offset, offset + 2), 16)) as Rgb;
}

/** Relative luminance per WCAG 2.1. */
export function luminance(color: Rgb | string): number {
  const [r, g, b] = typeof color === 'string' ? hexToRgb(color) : color;
  const channels = [r, g, b].map((raw) => {
    const value = raw / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

export function contrastRatio(foreground: Rgb | string, background: Rgb | string): number {
  const first = luminance(foreground);
  const second = luminance(background);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

/** CIE L*: the perceptual lightness a background pattern is judged by. */
export function lightness(color: Rgb | string): number {
  const y = luminance(color);
  return y > 0.008856 ? 116 * y ** (1 / 3) - 16 : 903.3 * y;
}

/** Composites `ink` over `background` at the given alpha. */
export function composite(ink: Rgb, alpha: number, background: Rgb): Rgb {
  return [0, 1, 2].map((channel) => ink[channel] * alpha + background[channel] * (1 - alpha)) as Rgb;
}
