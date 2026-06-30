import { DNS_TYPES } from "./constants";

export { queryDoh } from "@nodns/resolver";

export function dnsTypeNumberToString(num: number): string {
  return DNS_TYPES[num] ?? String(num);
}
