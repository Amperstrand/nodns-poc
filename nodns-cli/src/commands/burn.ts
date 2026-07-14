import { createCommand } from "commander";
import {
  createBurnRequest,
  pollForProof,
  DEFAULT_NOTARY_URL,
} from "@nodns/resolver";

export function createBurnCommand() {
  return createCommand("burn")
    .description("Create a proof-of-burn for a published Nostr event")
    .argument("<event_id>", "The hex event ID to burn for")
    .argument("<sats>", "Amount in sats to burn")
    .option("-u, --notary-url <url>", "Notary API URL", DEFAULT_NOTARY_URL)
    .option("-t, --timeout <seconds>", "Poll timeout in seconds", "360")
    .action(async (eventId: string, sats: string, opts: {
      notaryUrl: string;
      timeout: string;
    }) => {
      const valueSats = parseInt(sats, 10);
      if (isNaN(valueSats) || valueSats <= 0) {
        console.error("Invalid sats amount");
        process.exit(1);
      }

      console.log(`Creating burn request: ${valueSats} sats for event ${eventId.slice(0, 16)}...`);
      console.log(`Notary: ${opts.notaryUrl}`);

      let invoice;
      try {
        invoice = await createBurnRequest(
          { event_id: eventId, value_sats: valueSats },
          opts.notaryUrl,
        );
      } catch (e) {
        console.error(`Failed to create burn request: ${e}`);
        process.exit(1);
      }

      console.log(`\nLightning invoice:`);
      console.log(invoice.invoice);
      console.log(`\nPay this invoice to fund the burn. Waiting for payment...`);

      const timeoutSec = parseInt(opts.timeout, 10);
      const maxAttempts = Math.floor(timeoutSec / 3);

      try {
        const proof = await pollForProof(invoice, opts.notaryUrl, maxAttempts, 3000);
        console.log(`\n✓ Burn confirmed!`);
        console.log(`  TXID: ${proof.txid}`);
        console.log(`  Block: ${proof.block_height}`);
        console.log(`  Amount: ${proof.leaf_value / 1000} sats`);
        console.log(`  Event: ${proof.event_id}`);
        console.log(`\nThe notary will publish a kind 30021 proof to Nostr relays.`);
        console.log(`The bot will verify and accept it automatically.`);
      } catch (e) {
        console.error(`\n✗ ${e}`);
        process.exit(1);
      }
    });
}
