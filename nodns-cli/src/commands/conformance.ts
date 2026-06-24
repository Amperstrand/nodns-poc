import { Command } from "commander";
import {
  parseEventsToRecords,
  fetchZoneEvents,
  fetchApiRecords,
  generateZoneFile,
  compareRecords,
  formatRdata,
  type ZoneRecord,
} from "../lib/zone-file.js";
import { queryZoneRecords } from "../lib/dns.js";
import { DEFAULT_API_BASE } from "../lib/constants.js";

interface DiffResult {
  matchCount: number;
  onlyInA: ZoneRecord[];
  onlyInB: ZoneRecord[];
}

function normalizeRdata(type: string, rdata: string): string {
  const upper = type.toUpperCase();
  let normalized = rdata;

  if (upper === "TXT") {
    normalized = normalized.replace(/^"|"$/g, "").replace(/\\"/g, '"');
  }

  if (upper === "CNAME") {
    normalized = normalized.toLowerCase().replace(/\.$/, "");
  }

  if (upper === "MX") {
    const parts = normalized.trim().split(/\s+/);
    if (parts.length >= 2) {
      const priority = parts[0];
      const target = parts.slice(1).join(" ").toLowerCase().replace(/\.$/, "");
      normalized = `${priority} ${target}`;
    }
  }

  return normalized;
}

function recordCompareKey(record: ZoneRecord): string {
  const label = record.name.toLowerCase();
  const type = record.type.toUpperCase();
  const rdata = normalizeRdata(type, record.rdata);
  return `${label}|${type}|${rdata}`;
}

function diffRecordSets(a: ZoneRecord[], b: ZoneRecord[]): DiffResult {
  const mapA = new Map<string, ZoneRecord>();
  const mapB = new Map<string, ZoneRecord>();

  for (const r of a) {
    mapA.set(recordCompareKey(r), r);
  }
  for (const r of b) {
    mapB.set(recordCompareKey(r), r);
  }

  const onlyInA: ZoneRecord[] = [];
  const onlyInB: ZoneRecord[] = [];
  let matchCount = 0;

  for (const [key, record] of mapA) {
    if (mapB.has(key)) {
      matchCount++;
    } else {
      onlyInA.push(record);
    }
  }

  for (const [key, record] of mapB) {
    if (!mapA.has(key)) {
      onlyInB.push(record);
    }
  }

  onlyInA.sort(compareRecords);
  onlyInB.sort(compareRecords);

  return { matchCount, onlyInA, onlyInB };
}

function formatRecordLine(record: ZoneRecord): string {
  const label = record.name;
  const truncatedLabel =
    label.length > 30 ? `${label.slice(0, 27)}...` : label;
  const rdata = formatRdata(record.type, record.rdata);
  const truncatedRdata =
    rdata.length > 40 ? `${rdata.slice(0, 37)}...` : rdata;
  return `${truncatedLabel.padEnd(32)} ${record.type.padEnd(6)} ${truncatedRdata}`;
}

function generateUnifiedDiff(linesA: string[], linesB: string[]): string {
  const result: string[] = [];
  const setA = new Set(linesA);
  const setB = new Set(linesB);

  let i = 0;
  let j = 0;

  while (i < linesA.length || j < linesB.length) {
    const lineA = i < linesA.length ? linesA[i] : null;
    const lineB = j < linesB.length ? linesB[j] : null;

    if (lineA !== null && lineB !== null && lineA === lineB) {
      result.push(`  ${lineA}`);
      i++;
      j++;
    } else if (lineA !== null && !setB.has(lineA)) {
      result.push(`- ${lineA}`);
      i++;
    } else if (lineB !== null && !setA.has(lineB)) {
      result.push(`+ ${lineB}`);
      j++;
    } else if (lineA !== null && lineB !== null) {
      result.push(`- ${lineA}`);
      result.push(`+ ${lineB}`);
      i++;
      j++;
    } else if (lineA !== null) {
      result.push(`- ${lineA}`);
      i++;
    } else if (lineB !== null) {
      result.push(`+ ${lineB}`);
      j++;
    }
  }

  return result.join("\n");
}

export const conformanceCommand = new Command("conformance")
  .description("Cross-implementation conformance test")
  .argument("[zone]", "Zone name", "nodns.shop")
  .option("--api-base <url>", "Bot API base URL", DEFAULT_API_BASE)
  .action(async (zone: string, opts, cmd: Command) => {
    const o = cmd.optsWithGlobals();
    const relay = o.relay ?? "wss://relay.cashu.email";
    const relays = [relay];
    const apiBase = (opts.apiBase as string) || DEFAULT_API_BASE;

    console.log(`=== Conformance Test: ${zone} ===\n`);

    let sourceA: ZoneRecord[] = [];
    let sourceB: ZoneRecord[] = [];
    let sourceC: ZoneRecord[] = [];

    let relayOk = false;
    let apiOk = false;
    let dnsOk = false;

    try {
      const events = await fetchZoneEvents(relays);
      sourceA = parseEventsToRecords(events, zone);
      relayOk = true;
    } catch (e) {
      console.error(
        `  \u2717 Relay fetch failed: ${e instanceof Error ? e.message : String(e)}`,
      );
    }

    try {
      sourceB = await fetchApiRecords(apiBase, zone);
      apiOk = true;
    } catch (e) {
      console.error(
        `  \u2717 API fetch failed: ${e instanceof Error ? e.message : String(e)}`,
      );
    }

    if (apiOk && sourceB.length > 0) {
      try {
        const zoneSuffix = `.${zone}`;
        const knownFqdns = [...new Set(sourceB.map((r) => `${r.name}${zoneSuffix}`))];
        sourceC = await queryZoneRecords(zone, knownFqdns);
        dnsOk = true;
      } catch (e) {
        console.error(
          `  \u2717 DNS query failed: ${e instanceof Error ? e.message : String(e)}`,
        );
      }
    }

    console.log(
      `Source A (relay): ${relayOk ? `${sourceA.length} records` : "UNAVAILABLE"}`,
    );
    console.log(
      `Source B (API):   ${apiOk ? `${sourceB.length} records` : "UNAVAILABLE"}`,
    );
    console.log(
      `Source C (DNS):   ${dnsOk ? `${sourceC.length} records` : "UNAVAILABLE"}`,
    );
    console.log();

    let hasDifferences = false;

    if (relayOk && apiOk) {
      const diffAB = diffRecordSets(sourceA, sourceB);
      console.log("A vs B (relay vs API):");
      console.log(`  \u2713 ${diffAB.matchCount} records match`);

      if (diffAB.onlyInA.length > 0) {
        hasDifferences = true;
        console.log(
          `  \u26a0 ${diffAB.onlyInA.length} records ONLY in relay (not processed by bot):`,
        );
        for (const r of diffAB.onlyInA) {
          console.log(`    - ${formatRecordLine(r)}`);
        }
      }

      if (diffAB.onlyInB.length > 0) {
        hasDifferences = true;
        console.log(
          `  \u26a0 ${diffAB.onlyInB.length} records ONLY in API (deleted from relay?):`,
        );
        for (const r of diffAB.onlyInB) {
          console.log(`    - ${formatRecordLine(r)}`);
        }
      }
      console.log();
    }

    if (apiOk && dnsOk) {
      const diffBC = diffRecordSets(sourceB, sourceC);
      console.log("B vs C (API vs DNS):");
      console.log(`  \u2713 ${diffBC.matchCount} records match`);

      if (diffBC.onlyInA.length > 0) {
        hasDifferences = true;
        console.log(
          `  \u26a0 ${diffBC.onlyInA.length} records in API but MISSING from DNS:`,
        );
        for (const r of diffBC.onlyInA) {
          console.log(`    - ${formatRecordLine(r)}`);
        }
      }

      if (diffBC.onlyInB.length > 0) {
        hasDifferences = true;
        console.log(
          `  \u26a0 ${diffBC.onlyInB.length} records ONLY in DNS (stale?):`,
        );
        for (const r of diffBC.onlyInB) {
          console.log(`    - ${formatRecordLine(r)}`);
        }
      }
      console.log();
    }

    if (relayOk && apiOk) {
      const zoneA = generateZoneFile(sourceA, zone, { includeSoa: false });
      const zoneB = generateZoneFile(sourceB, zone, { includeSoa: false });

      if (zoneA !== zoneB) {
        console.log("Zone file diff (A vs B):");
        const diff = generateUnifiedDiff(
          zoneA.split("\n").filter((l) => l.length > 0),
          zoneB.split("\n").filter((l) => l.length > 0),
        );
        console.log(diff);
        console.log();
      }
    }

    const unprocessed =
      relayOk && apiOk ? diffRecordSets(sourceA, sourceB).onlyInA.length : 0;
    const dnsMissing =
      apiOk && dnsOk ? diffRecordSets(sourceB, sourceC).onlyInA.length : 0;

    if (!relayOk || !apiOk) {
      console.log(
        "VERDICT: \u2717 INCOMPLETE \u2014 one or more sources unavailable",
      );
    } else if (!hasDifferences) {
      console.log("VERDICT: \u2713 MATCH \u2014 all sources agree");
    } else {
      const issues: string[] = [];
      if (unprocessed > 0) issues.push(`${unprocessed} unprocessed events`);
      if (dnsMissing > 0) issues.push(`${dnsMissing} DNS-missing records`);
      console.log(`VERDICT: \u26a0 PARTIAL \u2014 ${issues.join(", ")}`);
    }
  });
