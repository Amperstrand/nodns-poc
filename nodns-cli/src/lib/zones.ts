import { SimplePool } from "nostr-tools/pool";
import type { NostrEvent } from "nostr-tools/pure";
import { parseZoneTxt, fetchDnsTxt } from "@nodns/resolver";
import { ZONE_HANDLER_KIND, QUERY_MAX_WAIT } from "./constants.js";
import type { ZoneInfo, ZonePricing, ZoneCheckOutcome } from "./types.js";

export { parseZoneTxt, fetchDnsTxt };

function buildZoneInfo(
  parsed: Record<string, string>,
  zone: string,
): ZoneInfo {
  const npub = parsed["npub"] ?? "";
  const testnet = parsed["testnet"] === "1" || parsed["testnet"] === "true";
  const npubNamesFree =
    parsed["npub_free"] === undefined
      ? true
      : parsed["npub_free"] === "true" || parsed["npub_free"] === "1";

  const createPrice = parseInt(parsed["create"] ?? "0", 10) || 0;
  const updatePrice = parseInt(parsed["update"] ?? "0", 10) || 0;
  const deletePrice = parseInt(parsed["delete"] ?? "0", 10) || 0;

  const pricing: ZonePricing | undefined =
    createPrice > 0 || updatePrice > 0 || deletePrice > 0
      ? { createPrice, updatePrice, deletePrice }
      : undefined;

  let mintUrl = parsed["mint"];
  if (mintUrl && !mintUrl.startsWith("http")) {
    mintUrl = `https://${mintUrl}`;
  }

  return {
    zone,
    npub,
    testnet,
    optedIn: true,
    pricing,
    mintUrl,
    npubNamesFree,
    handlerEventFound: false,
    verified: false,
  };
}

async function fetchZoneHandler(
  relays: string[],
  pubkeyHex: string,
): Promise<NostrEvent | null> {
  const pool = new SimplePool();
  try {
    const events = await pool.querySync(
      relays,
      {
        kinds: [ZONE_HANDLER_KIND],
        authors: [pubkeyHex],
        limit: 1,
      },
      { maxWait: QUERY_MAX_WAIT },
    );
    return events.length > 0 ? events[0] : null;
  } catch {
    return null;
  } finally {
    pool.close(relays);
  }
}

export async function checkZone(
  zone: string,
  relays: string[],
  skipCheck: boolean,
): Promise<ZoneCheckOutcome> {
  if (skipCheck) {
    return {
      result: "verified",
      info: {
        zone,
        npub: "",
        testnet: false,
        optedIn: true,
        npubNamesFree: true,
        handlerEventFound: true,
        verified: true,
      },
    };
  }

  const txt = await fetchDnsTxt(zone);
  if (!txt) {
    return { result: "not-opted-in" };
  }

  const parsed = parseZoneTxt(txt);
  const info = buildZoneInfo(parsed, zone);

  if (!info.npub) {
    return { result: "unverified", info };
  }

  const handler = await fetchZoneHandler(relays, info.npub);
  if (handler && handler.pubkey.toLowerCase() === info.npub.toLowerCase()) {
    info.handlerEventFound = true;
    info.verified = true;
    info.optedIn = true;
    if (info.testnet) {
      return { result: "testnet", info };
    }
    return { result: "verified", info };
  }

  info.handlerEventFound = handler !== null;
  info.verified = false;
  return { result: "unverified", info };
}
