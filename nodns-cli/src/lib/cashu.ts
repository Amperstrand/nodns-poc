import { Wallet, MintQuoteState, getEncodedToken } from "@cashu/cashu-ts";

const PAYMENT_TIMEOUT_MS = 300_000;
const POLL_INTERVAL_MS = 3_000;

export async function createPaymentToken(
  mintUrl: string,
  amountSats: number,
): Promise<string> {
  const wallet = new Wallet(mintUrl);
  await wallet.loadMint();

  const quote = await wallet.createMintQuoteBolt11(amountSats);

  console.error("\nPay this Lightning invoice:");
  console.error(`  ${quote.request}`);
  console.error("Waiting for payment (timeout: 300s)...\n");

  const startTime = Date.now();

  while (Date.now() - startTime < PAYMENT_TIMEOUT_MS) {
    const checked = await wallet.checkMintQuoteBolt11(quote.quote);
    if (checked.state === MintQuoteState.PAID) {
      const proofs = await wallet.mintProofsBolt11(amountSats, quote.quote);
      const { send } = await wallet.send(amountSats, proofs);
      return getEncodedToken({ mint: mintUrl, proofs: send });
    }
    await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
  }

  throw new Error("Payment timeout (300s). Please try again.");
}
