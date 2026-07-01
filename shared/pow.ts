export const DEFAULT_POW_DIFFICULTY = 20;

export const POB_PROOF_KIND = 30021;

export function countLeadingZeroBits(hexId: string): number {
  let count = 0;
  for (let i = 0; i < hexId.length; i++) {
    const nibble = parseInt(hexId[i], 16);
    if (isNaN(nibble)) break;
    if (nibble === 0) {
      count += 4;
    } else {
      count += Math.clz32(nibble) - 28;
      break;
    }
  }
  return count;
}
