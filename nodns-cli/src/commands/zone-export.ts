import { Command } from "commander";
import {
  parseEventsToRecords,
  generateZoneFile,
  fetchZoneEvents,
  fetchApiRecords,
  type ZoneRecord,
} from "../lib/zone-file.js";
import { DEFAULT_API_BASE, readRelays } from "../lib/constants.js";

export const zoneExportCommand = new Command("zone-export")
  .description("Export zone file from relay events or bot API")
  .argument("[zone]", "Zone name", "nodns.shop")
  .option("--from-api <url>", "Fetch from bot API instead of relay", "")
  .option("--api-base <url>", "Bot API base URL", DEFAULT_API_BASE)
  .option("--json", "Output as JSON instead of BIND zone file")
  .option("--no-soa", "Omit SOA record from zone file")
  .action(async (zone: string, opts, cmd: Command) => {
    const o = cmd.optsWithGlobals();
    const relays = readRelays(o.relay);
    const jsonOutput = opts.json as boolean;
    const includeSoa = opts.soa !== false;
    const apiBase = (opts.apiBase as string) || DEFAULT_API_BASE;
    const fromApi = (opts.fromApi as string) || "";

    let records: ZoneRecord[];

    if (fromApi) {
      const baseUrl = fromApi || apiBase;
      try {
        records = await fetchApiRecords(baseUrl, zone);
      } catch (e) {
        console.error(
          `Error fetching from API: ${e instanceof Error ? e.message : String(e)}`,
        );
        process.exit(1);
      }
    } else {
      let events;
      try {
        events = await fetchZoneEvents(relays);
      } catch (e) {
        console.error(
          `Error fetching from relay: ${e instanceof Error ? e.message : String(e)}`,
        );
        process.exit(1);
      }
      records = parseEventsToRecords(events, zone);
    }

    if (jsonOutput) {
      console.log(
        JSON.stringify({ zone, count: records.length, records }, null, 2),
      );
      return;
    }

    const zoneFile = generateZoneFile(records, zone, { includeSoa });
    process.stdout.write(zoneFile);
  });
