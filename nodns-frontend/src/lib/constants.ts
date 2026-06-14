export const API_BASE = process.env.NEXT_PUBLIC_API_BASE ?? '';

export const RELAYS = [
  'wss://relay.damus.io',
  'wss://nos.lol',
  'wss://nostr.wine',
  'wss://relay.ngit.dev',
  'wss://relay.tollgate.me',
];

export const DEFAULT_ZONE = 'nodns.shop';

export const DNS_TYPES: Record<number, string> = {
  1: 'A',
  28: 'AAAA',
  5: 'CNAME',
  16: 'TXT',
  15: 'MX',
  2: 'NS',
  6: 'SOA',
};

export const DNS_STATUS_CODES: Record<number, string> = {
  0: 'No Error',
  1: 'Form Error',
  2: 'Server Failure',
  3: 'NXDOMAIN',
  4: 'Not Implemented',
  5: 'Refused',
};
