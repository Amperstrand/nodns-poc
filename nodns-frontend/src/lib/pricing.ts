import { DEFAULT_ZONE } from "./constants";

/**
 * Price in sats based on subdomain label length.
 * Matches backend pricing tiers.
 */
export function getPriceForName(name: string, basePrice: number = 2): number {
  const len = name.length;
  if (len <= 3) return basePrice * 100;
  if (len <= 6) return basePrice * 10;
  return basePrice * 2;
}

/** Sanitize a subdomain label: lowercase, strip dots/spaces. */
export function sanitizeName(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/\.nodns\.shop\.?$/i, "")
    .replace(/[^a-z0-9-]/g, "");
}

/** Build the full FQDN. */
export function toFqdn(name: string): string {
  return `${name}.${DEFAULT_ZONE}`;
}
