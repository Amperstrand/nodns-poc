export { createResolver } from './resolver.js';
export type { Resolver } from './resolver.js';

export {
  queryDoh,
  queryDnsRecords,
  queryAllDnsRecordTypes,
} from './dns.js';

export {
  parseRecordsFromEvent,
  parseRecords,
  parseRecord,
  checkValidity,
  computeFqdn,
  isNpubDerivedName,
  deduplicateRecords,
} from './parse.js';

export {
  queryRecordsByPubkey,
  queryRecordsByDomain,
  queryAllRecentRecords,
  fetchEvents,
  pubkeyToNpub,
  generateKeypair,
  decodeNsec,
  decodeSec,
  buildRecordTag,
  buildDeleteTag,
  buildCashuTag,
} from './nostr.js';

export type { Keypair } from './nostr.js';

export {
  fetchTripartiteRecords,
  compareTripartite,
  fetchApiRecords,
  fetchNostrRecords,
  fetchDnsRecords,
  toResolvedRecords,
} from './verify.js';

export {
  discoverZones,
  fetchDnsTxt,
  parseZoneTxt,
  checkZone,
} from './zones.js';

export {
  validateRecordName,
  validateRecordData,
  validateReservedTxt,
  validateRecord,
  validateDomainName,
  validateNsec,
} from './validation.js';

export type {
  DnsRecordType,
  ResolutionMode,
  DohAnswer,
  DohResponse,
  DnsAnswer,
  DnsRecord,
  NostrDnsRecord,
  RecordInfo,
  ResolvedRecord,
  SourceStatus,
  SourceResult,
  TripartiteRecords,
  TripartiteComparison,
  ResolveVerifiedResult,
  ReverseResult,
  SpecVersion,
  ValidityInfo,
  ZoneStatusLevel,
  ZonePricing,
  DiscoveredZone,
  ZoneCheckOutcome,
  ZoneInfo,
  ResolverOptions,
  NostrEvent,
} from './types.js';

export {
  RECORD_KIND,
  ZONE_HANDLER_KIND,
  DEFAULT_ZONE,
  DEFAULT_API_BASE,
  DEFAULT_DOH_ENDPOINT,
  VALID_RECORD_TYPES,
  DNS_TYPE_MAP,
} from './types.js';
