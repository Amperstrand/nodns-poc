import { DNS_TYPES, type DnsType } from "./constants.js";

const PRIVATE_IP_RANGES = [
  /^10\./,
  /^172\.(1[6-9]|2[0-9]|3[01])\./,
  /^192\.168\./,
  /^127\./,
  /^169\.254\./,
];

const IPV6_PRIVATE = [
  /^fc[0-9a-f]{2}:/i,
  /^fd[0-9a-f]{2}:/i,
  /^::1$/,
  /^fe80:/i,
];

export function validateRecord(
  type: string,
  name: string,
  value: string,
): string | null {
  const upperType = type.toUpperCase();
  if (!DNS_TYPES.includes(upperType as DnsType)) {
    return `Record type ${type} is not supported`;
  }

  if (upperType === "A") {
    const ip = value.trim();
    if (!/^\d{1,3}(\.\d{1,3}){3}$/.test(ip)) {
      return "Invalid IPv4 address";
    }
    for (const range of PRIVATE_IP_RANGES) {
      if (range.test(ip)) return "Private IP addresses are not allowed";
    }
  }

  if (upperType === "AAAA") {
    const ip = value.trim();
    if (!/^[0-9a-f:]+$/i.test(ip)) {
      return "Invalid IPv6 address";
    }
    for (const range of IPV6_PRIVATE) {
      if (range.test(ip)) return "Private IPv6 addresses are not allowed";
    }
  }

  if (upperType === "CNAME") {
    if (!value.trim()) return "CNAME value cannot be empty";
    if (value.length > 253) return "CNAME value too long";
  }

  if (upperType === "TXT") {
    if (value.length > 512) return "TXT record exceeds 512 characters";
  }

  if (upperType === "MX") {
    const parts = value.trim().split(/\s+/);
    if (parts.length < 2) return "MX record needs priority and host (e.g. '10 mail.example.com.')";
    const priority = parseInt(parts[0], 10);
    if (isNaN(priority) || priority < 0 || priority > 65535) {
      return "MX priority must be 0-65535";
    }
  }

  if (name && !/^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/.test(name)) {
    return "Invalid subdomain name (lowercase alphanumeric and hyphens only)";
  }

  return null;
}
