export const API_BASE = process.env.NEXT_PUBLIC_API_BASE ?? '';

import { PUBLISH_RELAYS as _PUBLISH, READ_RELAYS as _READ } from '../../../shared/relays';
import { DEFAULT_POW_DIFFICULTY as _POW } from '../../../shared/pow';

export const RELAYS = _READ;
export const PUBLISH_RELAYS = _PUBLISH;
export const DEFAULT_POW_DIFFICULTY = _POW;

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

export function statusDot(status: string): string {
  if (status === 'ok') return '🟢';
  if (status === 'error') return '🔴';
  if (status === 'loading') return '🟡';
  return '⚫';
}
