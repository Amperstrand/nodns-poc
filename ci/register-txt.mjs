import { generateSecretKey, getPublicKey, nip19, finalizeEvent } from "nostr-tools";
import { Relay } from "nostr-tools/relay";
import { Wallet, getEncodedToken } from "@cashu/cashu-ts";
import { WebSocket } from "ws";

globalThis.WebSocket = WebSocket;

const MINT_URL = "https://testnut.cashu.space";
const ZONE = "nodns.shop";
const RELAYS = [
  "wss://relay.damus.io",
  "wss://nos.lol",
  "wss://relay.nostr.band",
];
const SATS = 4;

function log(msg) {
  console.log(`[nodns-ci] ${msg}`);
}

async function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function randomLabel() {
  const hex = Math.random().toString(16).slice(2, 8);
  return `ci-${hex}`;
}

async function mintCashu(amount) {
  log(`Minting ${amount} sats from testnut...`);
  const wallet = new Wallet(MINT_URL, undefined);
  await wallet.loadMint();

  const quote = await wallet.createMintQuoteBolt11(amount);
  log(`Quote: ${quote.quote}`);

  for (let i = 0; i < 15; i++) {
    await sleep(2000);
    const checked = await wallet.checkMintQuoteBolt11(quote.quote);
    log(`Mint state ${i + 1}: ${checked.state}`);
    if (checked.state === "PAID") break;
    if (i === 14) throw new Error("Mint quote never settled");
  }

  const proofs = await wallet.mintProofsBolt11(amount, quote.quote);
  const token = getEncodedToken({ mint: MINT_URL, proofs });
  log(`Token created (${proofs.reduce((s, p) => s + Number(p.amount), 0)} sats)`);
  return token;
}

async function publishEvent(event) {
  const relays = ["wss://relay.damus.io", "wss://nos.lol"];
  for (const url of relays) {
    try {
      const relay = await Relay.connect(url);
      await relay.publish(event);
      relay.close();
      log(`Published to ${url}`);
      return;
    } catch (e) {
      log(`Failed ${url}: ${e.message}`);
    }
  }
  throw new Error("Failed to publish to any relay");
}

async function main() {
  log("=== NoDNS CI: Register TXT record ===");

  // 1. Generate ephemeral nsec
  const sk = generateSecretKey();
  const pk = getPublicKey(sk);
  const npub = nip19.npubEncode(pk);
  log(`Generated nsec. npub: ${npub}`);

  // 2. Mint Cashu tokens (auto-settled on testnut)
  const cashuToken = await mintCashu(SATS);

  // 3. Build Nostr event
  const label = randomLabel();
  const fqdn = `${label}.${npub}.${ZONE}`;
  const txtValue = `ci-test-${Date.now()}`;
  log(`Registering: ${fqdn} → TXT "${txtValue}"`);

  const eventTemplate = {
    kind: 11111,
    created_at: Math.floor(Date.now() / 1000),
    tags: [
      [
        "record", "TXT", "", txtValue,
        "", "", "", "", "", "", "3600",
      ],
      ["cashu", cashuToken, MINT_URL, String(SATS)],
    ],
    content: "",
  };

  const event = finalizeEvent(eventTemplate, sk);
  log(`Event signed: kind=${event.kind}, id=${event.id.slice(0, 16)}...`);

  // 4. Publish to relays
  await publishEvent(event);

  // 5. Wait for bot to process
  log("Waiting 20s for bot...");
  await sleep(20000);

  log("Checking API...");
  const resp = await fetch(`https://nodns.shop/api/records/by-npub/${npub}`);
  const data = await resp.json();
  const records = data.records || [];
  const hit = records.find(
    (r) => r.type === "TXT" && r.rdata === txtValue,
  );

  if (hit) {
    log(`PASS: ${hit.fqdn} TXT "${hit.rdata}"`);
    process.exit(0);
  }

  log(`Records found: ${records.length}`);
  for (const r of records.slice(0, 5)) {
    log(`  ${r.fqdn} ${r.rtype} "${r.rdata}"`);
  }
  log("FAIL: record not found — payment may have been rejected");
  process.exit(1);
}

main().catch((err) => {
  console.error("[nodns-ci] FATAL:", err);
  process.exit(1);
});
