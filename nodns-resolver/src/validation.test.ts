import { describe, it, expect } from 'vitest';
import {
  validateRecordName,
  validateRecordData,
  validateReservedTxt,
  validateRecord,
  validateDomainName,
  validateNsec,
} from './validation.js';

describe('validateRecordName', () => {
  it('accepts empty string and @ as apex', () => {
    expect(validateRecordName('')).toBeNull();
    expect(validateRecordName('@')).toBeNull();
  });

  it('accepts valid lowercase labels', () => {
    expect(validateRecordName('alice')).toBeNull();
    expect(validateRecordName('my-sub')).toBeNull();
    expect(validateRecordName('abc123')).toBeNull();
  });

  it('rejects names over 63 chars', () => {
    expect(validateRecordName('a'.repeat(64))).toContain('63');
  });

  it('rejects names starting with hyphen', () => {
    expect(validateRecordName('-abc')).toContain('hyphen');
  });

  it('rejects names ending with hyphen', () => {
    expect(validateRecordName('abc-')).toContain('hyphen');
  });

  it('rejects uppercase and special chars', () => {
    expect(validateRecordName('Alice')).toContain('lowercase');
    expect(validateRecordName('abc_def')).toContain('lowercase');
  });
});

describe('validateRecordData', () => {
  it('validates A records', () => {
    expect(validateRecordData('A', '1.2.3.4')).toBeNull();
    expect(validateRecordData('A', '8.8.8.8')).toBeNull();
  });

  it('rejects invalid A records', () => {
    expect(validateRecordData('A', '')).toContain('IP');
    expect(validateRecordData('A', '999.1.1.1')).toContain('IPv4');
    expect(validateRecordData('A', 'not-an-ip')).toContain('IPv4');
  });

  it('blocks private IPs in A records', () => {
    expect(validateRecordData('A', '10.0.0.1')).toContain('Private');
    expect(validateRecordData('A', '192.168.1.1')).toContain('Private');
    expect(validateRecordData('A', '172.16.0.1')).toContain('Private');
    expect(validateRecordData('A', '127.0.0.1')).toContain('Private');
  });

  it('validates TXT records', () => {
    expect(validateRecordData('TXT', 'hello world')).toBeNull();
    expect(validateRecordData('TXT', 'a'.repeat(512))).toBeNull();
  });

  it('rejects TXT over 512 chars', () => {
    expect(validateRecordData('TXT', 'a'.repeat(513))).toContain('512');
  });

  it('validates CNAME records', () => {
    expect(validateRecordData('CNAME', 'example.com')).toBeNull();
  });

  it('rejects empty CNAME', () => {
    expect(validateRecordData('CNAME', '')).toContain('target');
  });

  it('validates MX records', () => {
    expect(validateRecordData('MX', '10 mail.example.com')).toBeNull();
  });

  it('rejects MX without priority', () => {
    expect(validateRecordData('MX', 'mail.example.com')).toContain('priority');
  });

  it('validates AAAA records', () => {
    expect(validateRecordData('AAAA', '2001:db8::1')).toBeNull();
  });

  it('rejects truly unsupported types', () => {
    expect(validateRecordData('FOO', 'test')).toContain('Unsupported');
  });
});

describe('validateReservedTxt', () => {
  it('blocks DMARC', () => {
    expect(validateReservedTxt('TXT', '_dmarc', 'v=DMARC1')).toContain('DMARC');
  });

  it('blocks DKIM', () => {
    expect(validateReservedTxt('TXT', '_domainkey', 'v=DKIM1')).toContain('DKIM');
  });

  it('blocks SPF at apex', () => {
    expect(validateReservedTxt('TXT', '@', 'v=spf1 -all')).toContain('SPF');
    expect(validateReservedTxt('TXT', '', 'v=spf1 -all')).toContain('SPF');
  });

  it('allows non-reserved TXT', () => {
    expect(validateReservedTxt('TXT', 'alice', 'hello')).toBeNull();
  });

  it('ignores non-TXT records', () => {
    expect(validateReservedTxt('A', '_dmarc', '1.2.3.4')).toBeNull();
  });
});

describe('validateRecord (combined)', () => {
  it('passes for valid record', () => {
    expect(validateRecord('A', '', '1.2.3.4')).toBeNull();
    expect(validateRecord('TXT', 'alice', 'hello')).toBeNull();
  });

  it('fails for invalid name', () => {
    expect(validateRecord('A', '-bad', '1.2.3.4')).toContain('hyphen');
  });

  it('fails for invalid data', () => {
    expect(validateRecord('A', '', 'not-an-ip')).toContain('IPv4');
  });

  it('fails for reserved TXT via validateReservedTxt', () => {
    expect(validateReservedTxt('TXT', '_dmarc', 'v=DMARC1')).toContain('DMARC');
    expect(validateReservedTxt('TXT', '_domainkey', 'v=DKIM1')).toContain('DKIM');
    expect(validateReservedTxt('TXT', '@', 'v=spf1 -all')).toContain('SPF');
  });
});

describe('validateDomainName', () => {
  it('accepts valid domain names', () => {
    expect(validateDomainName('alice')).toBeNull();
    expect(validateDomainName('my-cool-name')).toBeNull();
  });

  it('rejects empty', () => {
    expect(validateDomainName('')).toContain('empty');
  });

  it('rejects over 63 chars', () => {
    expect(validateDomainName('a'.repeat(64))).toContain('63');
  });

  it('rejects invalid chars', () => {
    expect(validateDomainName('Alice')).toContain('lowercase');
    expect(validateDomainName('under_score')).toContain('lowercase');
  });
});

describe('validateNsec', () => {
  it('accepts valid nsec prefix', () => {
    expect(validateNsec('nsec1' + 'a'.repeat(36))).toBeNull();
  });

  it('rejects wrong prefix', () => {
    expect(validateNsec('npub1' + 'a'.repeat(36))).toContain('nsec1');
  });

  it('rejects too short', () => {
    expect(validateNsec('nsec1short')).toContain('short');
  });
});
