/**
 * Client-side DNS record validation mirroring backend rules in parser.rs.
 *
 * Every function returns an error message string on failure, or null on success.
 */

const IPV4_RE = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;

// Simplified IPv6 regex — handles full, compressed, and mixed forms
const IPV6_RE =
  /^(([0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}|(::([0-9a-fA-F]{1,4}:){0,5}[0-9a-fA-F]{0,4})|(([0-9a-fA-F]{1,4}:){1,5}:([0-9a-fA-F]{1,4}:){0,3}[0-9a-fA-F]{0,4})|(([0-9a-fA-F]{1,4}:){1,4}:([0-9a-fA-F]{1,4}:){0,2}[0-9a-fA-F]{0,4})|(([0-9a-fA-F]{1,4}:){1,3}:([0-9a-fA-F]{1,4}:){0,1}[0-9a-fA-F]{0,4})|(([0-9a-fA-F]{1,4}:){1,2}:[0-9a-fA-F]{0,4})|([0-9a-fA-F]{1,4}::([0-9a-fA-F]{1,4}:){0,1}[0-9a-fA-F]{0,4})|(::[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,2})|(::))$/i;

// Domain-like: at least one dot with non-empty labels, or a single label (for things like "localhost")
const DOMAIN_RE = /^[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*$/;

/**
 * Validate a DNS record name (subdomain label).
 * Empty string or "@" means apex — always valid.
 */
export function validateRecordName(name: string): string | null {
  // Empty or "@" is valid (apex domain)
  if (name === "" || name === "@") return null;

  if (name.length > 63) {
    return "Name must be 63 characters or fewer.";
  }
  if (name.startsWith("-")) {
    return "Name cannot start with a hyphen.";
  }
  if (name.endsWith("-")) {
    return "Name cannot end with a hyphen.";
  }
  if (!/^[a-z0-9-]+$/.test(name)) {
    return "Name must be lowercase alphanumeric with hyphens only.";
  }
  return null;
}

/**
 * Validate DNS record data based on the record type.
 * Mirrors the type-specific validation in parser.rs `validate_record()`.
 */
export function validateRecordData(type: string, rdata: string): string | null {
  const fields = rdata.trim().split(/\s+/);

  switch (type) {
    case "A": {
      if (!rdata.trim()) return "A record requires an IP address.";
      const match = IPV4_RE.exec(rdata.trim());
      if (!match) return "Invalid IPv4 address.";
      const octets = [Number(match[1]), Number(match[2]), Number(match[3]), Number(match[4])];
      if (octets.some((o) => o > 255)) return "Invalid IPv4 address.";
      return null;
    }

    case "AAAA": {
      if (!rdata.trim()) return "AAAA record requires an IP address.";
      if (!IPV6_RE.test(rdata.trim())) return "Invalid IPv6 address.";
      return null;
    }

    case "CNAME":
    case "NS":
    case "PTR": {
      if (!rdata.trim()) return `${type} record requires a target domain.`;
      if (!DOMAIN_RE.test(rdata.trim())) {
        return `${type} record must be a valid domain name.`;
      }
      return null;
    }

    case "TXT": {
      if (rdata.length > 512) {
        return `TXT record must not exceed 512 characters (currently ${rdata.length}).`;
      }
      return null;
    }

    case "MX": {
      if (fields.length < 2) {
        return "MX record requires: priority hostname";
      }
      const priority = Number(fields[0]);
      if (!Number.isFinite(priority) || !Number.isInteger(priority) || priority < 0 || priority > 65535) {
        return "MX priority must be a number (0-65535).";
      }
      if (!DOMAIN_RE.test(fields[1])) {
        return "MX hostname must be a valid domain name.";
      }
      return null;
    }

    case "SRV": {
      if (fields.length < 4) {
        return "SRV record requires: priority weight port target";
      }
      for (let i = 0; i < 3; i++) {
        const val = Number(fields[i]);
        const label = ["priority", "weight", "port"][i];
        if (!Number.isFinite(val) || !Number.isInteger(val) || val < 0 || val > 65535) {
          return `SRV ${label} must be a number (0-65535).`;
        }
      }
      return null;
    }

    default:
      return `Unsupported record type: ${type}`;
  }
}

/**
 * Validate reserved/blocked TXT records.
 * Prevents users from creating records that interfere with email security.
 */
export function validateReservedTxt(
  type: string,
  name: string,
  rdata: string,
): string | null {
  if (type !== "TXT") return null;

  const normalizedName = name === "" ? "@" : name;
  const trimmedRdata = rdata.trimStart();

  if (normalizedName === "_dmarc") {
    return "TXT records at _dmarc are reserved (DMARC policy).";
  }
  if (normalizedName.startsWith("_domainkey")) {
    return 'TXT records starting with "_domainkey" are reserved (DKIM).';
  }
  if (
    (normalizedName === "@" || normalizedName === "") &&
    trimmedRdata.startsWith("v=spf1")
  ) {
    return 'TXT records with "v=spf1" at the apex are reserved (SPF).';
  }

  return null;
}

/**
 * Run all validations for a single record and return the first error or null.
 */
export function validateRecord(
  type: string,
  name: string,
  rdata: string,
): string | null {
  return (
    validateRecordName(name) ??
    validateRecordData(type, rdata) ??
    validateReservedTxt(type, name, rdata)
  );
}
