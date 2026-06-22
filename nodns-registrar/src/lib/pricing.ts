export function calculatePrice(nameLength: number, isNpub: boolean): number {
  if (isNpub) return 0;
  if (nameLength <= 3) return 100;
  if (nameLength <= 6) return 50;
  if (nameLength <= 10) return 20;
  return 10;
}

export function formatSats(sats: number): string {
  if (sats === 0) return "free";
  return `${sats} sats`;
}
