import { discoverZones as sdkDiscoverZones } from "@nodns/resolver";
import { RELAYS } from "./constants";
import type { ZoneStatus } from "./types";

export { parseZoneTxt, fetchDnsTxt } from "@nodns/resolver";

export async function discoverZones(): Promise<ZoneStatus[]> {
  return sdkDiscoverZones(RELAYS);
}
