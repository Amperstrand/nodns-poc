import { discoverZones as sdkDiscoverZones } from "@nodns/resolver";
import { RELAYS } from "./constants";

export { parseZoneTxt, fetchDnsTxt } from "@nodns/resolver";
export type { DiscoveredZone } from "@nodns/resolver";

export async function discoverZones() {
  return sdkDiscoverZones(RELAYS);
}
