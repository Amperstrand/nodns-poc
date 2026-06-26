import {
  Wallet,
  MintQuoteState,
  Amount,
  getEncodedToken,
  getDecodedToken,
  parseP2PKSecret,
  type P2PKOptions as CashuP2PKOptions,
  type OutputConfig,
} from "@cashu/cashu-ts";
import { createPaymentToken } from "./cashu.js";

const PAYMENT_TIMEOUT_MS = 300_000;
const POLL_INTERVAL_MS = 3_000;
const SECONDS_PER_DAY = 86_400;

export interface P2PKTokenOptions {
  zonePubkeyHex: string;
  userPubkeyHex: string;
  refundAfterDays: number;
  amountSats: number;
  mintUrl: string;
}

export interface P2PKTokenResult {
  token: string;
  refundDate: Date;
  zonePubkey: string;
  p2pk: boolean;
}

export interface RefundCheckResult {
  isP2PK: boolean;
  eligible: boolean;
  userIsRefundKey: boolean;
  refundDate?: Date;
  locktime?: number;
  lockPubkey?: string;
  refundPubkeys: string[];
  mint?: string;
}

function nostrToCompressed(hex: string): string {
  return `02${hex}`;
}

function extractLocktime(tags: string[][] | undefined): number | undefined {
  if (!tags) return undefined;
  for (const tag of tags) {
    if (tag[0] === "locktime" && tag[1]) {
      const ts = parseInt(tag[1], 10);
      return isNaN(ts) ? undefined : ts;
    }
  }
  return undefined;
}

function extractRefundKeys(tags: string[][] | undefined): string[] {
  if (!tags) return [];
  const keys: string[] = [];
  for (const tag of tags) {
    if (tag[0] === "refund" && tag[1]) {
      keys.push(tag[1]);
    }
  }
  return keys;
}

export async function mintSupportsP2PK(mintUrl: string): Promise<boolean> {
  try {
    const wallet = new Wallet(mintUrl);
    await wallet.loadMint();
    const info = wallet.getMintInfo();
    return info.nuts["11"]?.supported === true;
  } catch {
    return false;
  }
}

export async function createP2pkTokenWithRefund(
  opts: P2PKTokenOptions,
): Promise<P2PKTokenResult> {
  const p2pkSupported = await mintSupportsP2PK(opts.mintUrl);

  if (!p2pkSupported) {
    console.error("⚠ Mint does not advertise NUT-11 (P2PK) support.");
    console.error("  Falling back to regular (unlocked) token.\n");
    const token = await createPaymentToken(opts.mintUrl, opts.amountSats);
    const fallbackRefund = new Date(
      Date.now() + opts.refundAfterDays * SECONDS_PER_DAY * 1000,
    );
    return {
      token,
      refundDate: fallbackRefund,
      zonePubkey: nostrToCompressed(opts.zonePubkeyHex),
      p2pk: false,
    };
  }

  const wallet = new Wallet(opts.mintUrl);
  await wallet.loadMint();

  const quote = await wallet.createMintQuoteBolt11(opts.amountSats);

  console.error("\nPay this Lightning invoice:");
  console.error(`  ${quote.request}`);
  console.error("Waiting for payment (timeout: 300s)...\n");

  const startTime = Date.now();

  while (Date.now() - startTime < PAYMENT_TIMEOUT_MS) {
    const checked = await wallet.checkMintQuoteBolt11(quote.quote);
    if (checked.state === MintQuoteState.PAID) break;
    await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
  }

  if (Date.now() - startTime >= PAYMENT_TIMEOUT_MS) {
    throw new Error("Payment timeout (300s). Please try again.");
  }

  const proofs = await wallet.mintProofsBolt11(opts.amountSats, quote.quote);

  const zoneCompressed = nostrToCompressed(opts.zonePubkeyHex);
  const userCompressed = nostrToCompressed(opts.userPubkeyHex);
  const refundTimestamp =
    Math.floor(Date.now() / 1000) + opts.refundAfterDays * SECONDS_PER_DAY;

  const p2pkOptions: CashuP2PKOptions = {
    pubkey: zoneCompressed,
    locktime: refundTimestamp,
    refundKeys: [userCompressed],
  };

  const outputConfig: OutputConfig = {
    send: { type: "p2pk", options: p2pkOptions },
  };

  const { send } = await wallet.send(
    opts.amountSats,
    proofs,
    undefined,
    outputConfig,
  );

  const token = getEncodedToken({ mint: opts.mintUrl, proofs: send });

  return {
    token,
    refundDate: new Date(refundTimestamp * 1000),
    zonePubkey: zoneCompressed,
    p2pk: true,
  };
}

export function checkRefundEligibility(
  token: string,
  userPubkeyHex?: string,
): RefundCheckResult {
  let decoded;
  try {
    decoded = getDecodedToken(token, []);
  } catch {
    return {
      isP2PK: false,
      eligible: false,
      userIsRefundKey: false,
      refundPubkeys: [],
    };
  }

  const now = Math.floor(Date.now() / 1000);
  let userCompressed: string | undefined;
  if (userPubkeyHex) {
    userCompressed = nostrToCompressed(userPubkeyHex);
  }

  for (const proof of decoded.proofs) {
    try {
      const secret = parseP2PKSecret(proof.secret);
      const secretData = secret[1];
      const tags = secretData.tags;

      const locktime = extractLocktime(tags);
      const refundKeys = extractRefundKeys(tags);

      const userIsRefundKey =
        userCompressed !== undefined
          ? refundKeys.includes(userPubkeyHex!) ||
            refundKeys.includes(userCompressed)
          : false;

      if (locktime === undefined) {
        return {
          isP2PK: true,
          eligible: false,
          userIsRefundKey,
          lockPubkey: secretData.data,
          refundPubkeys: refundKeys,
          mint: decoded.mint,
        };
      }

      const eligible = now >= locktime;

      return {
        isP2PK: true,
        eligible,
        userIsRefundKey,
        refundDate: new Date(locktime * 1000),
        locktime,
        lockPubkey: secretData.data,
        refundPubkeys: refundKeys,
        mint: decoded.mint,
      };
    } catch {
      continue;
    }
  }

  return {
    isP2PK: false,
    eligible: false,
    userIsRefundKey: false,
    refundPubkeys: [],
    mint: decoded.mint,
  };
}

export async function reclaimExpiredToken(
  token: string,
  userPrivkeyHex: string,
  mintUrl: string,
): Promise<string> {
  const wallet = new Wallet(mintUrl);
  await wallet.loadMint();

  const proofs = await wallet.receive(token, {
    privkey: userPrivkeyHex,
  });

  const total = proofs.reduce(
    (sum, p) => sum.add(p.amount),
    Amount.zero(),
  );

  const { send } = await wallet.send(total, proofs);

  return getEncodedToken({ mint: mintUrl, proofs: send });
}
