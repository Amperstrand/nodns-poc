import { SimplePool } from "nostr-tools/pool";
import type { NostrEvent } from "nostr-tools/pure";
import {
  ZONE_HANDLER_KIND,
  DOH_ENDPOINT,
  QUERY_MAX_WAIT,
} from "./constants.js";
import type { ZoneInfo, ZonePricing, ZoneCheckOutcome } from "./types.js";

export function parseZoneTxt(txt: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const part of txt.split(";")) {
    const eq = part.indexOf("=");
    if (eq === -1) continue;
    const key = part.slice(0, eq).trim();
    const val = part.slice(eq + 1).trim();
    if (key) result[key] = val;
  }
  return result;
}

interface DohAnswer {
  name: string;
  type: number;
  TTL: number;
  data: string;
}

interface DohResponse {
  Status: number;
  Answer?: DohAnswer[];
}

function isDohResponse(data: unknown): data is DohResponse {
  return typeof data === "object" && data !== null && "Status" in data;
}

export async function fetchDnsTxt(zone: string): Promise<string | null> {
  const name = `_nodns.${zone}`;
  const url = `${DOH_ENDPOINT}?name=${encodeURIComponent(name)}&type=TXT`;
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 10_000);
  try {
    const res = await fetch(url, {
      headers: { accept: "application/dns-json" },
      signal: controller.signal,
    });
    if (!res.ok) return null;
    const data: unknown = await res.json();
    if (!isDohResponse(data) || !data.Answer) return null;
    for (const answer of data.Answer) {
      if (answer.type === 16 && typeof answer.data === "string") {
        return answer.data
          .replace(/^"/, "")
          .replace(/"$/, "")
          .replace(/"\s*"/g, "");
      }
    }
    return null;
  } catch {
    return null;
  } finally {
    clearTimeout(timeout);
  }
}

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
