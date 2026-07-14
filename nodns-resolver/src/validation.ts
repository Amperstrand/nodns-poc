const IPV4_RE = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;

const IPV6_RE =
  /^(([0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}|(::([0-9a-fA-F]{1,4}:){0,5}[0-9a-fA-F]{0,4})|(([0-9a-fA-F]{1,4}:){1,5}:([0-9a-fA-F]{1,4}:){0,3}[0-9a-fA-F]{0,4})|(([0-9a-fA-F]{1,4}:){1,4}:([0-9a-fA-F]{1,4}:){0,2}[0-9a-fA-F]{0,4})|(([0-9a-fA-F]{1,4}:){1,3}:([0-9a-fA-F]{1,4}:){0,1}[0-9a-fA-F]{0,4})|(([0-9a-fA-F]{1,4}:){1,2}:[0-9a-fA-F]{0,4})|([0-9a-fA-F]{1,4}::([0-9a-fA-F]{1,4}:){0,1}[0-9a-fA-F]{0,4})|(::[0-9a-fA-F]{1,4}(:[0-9a-fA-F]{1,4}){0,2})|(::))$/i;

const DOMAIN_RE = /^[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*$/;

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

export function validateRecordName(name: string): string | null {
  if (name === "" || name === "@") return null;
  if (name.length > 63) return "Name must be 63 characters or fewer.";
  if (name.startsWith("-")) return "Name cannot start with a hyphen.";
  if (name.endsWith("-")) return "Name cannot end with a hyphen.";
  if (!/^[a-z0-9-]+$/.test(name))
    return "Name must be lowercase alphanumeric with hyphens only.";
  return null;
}

export function validateRecordData(type: string, rdata: string): string | null {
  const fields = rdata.trim().split(/\s+/);
  const upperType = type.toUpperCase();

  switch (upperType) {
    case "A": {
      if (!rdata.trim()) return "A record requires an IP address.";
      const match = IPV4_RE.exec(rdata.trim());
      if (!match) return "Invalid IPv4 address.";
      const octets = [Number(match[1]), Number(match[2]), Number(match[3]), Number(match[4])];
      if (octets.some((o) => o > 255)) return "Invalid IPv4 address.";
      const ip = rdata.trim();
      for (const range of PRIVATE_IP_RANGES) {
        if (range.test(ip)) return "Private IP addresses are not allowed.";
      }
      return null;
    }
    case "AAAA": {
      if (!rdata.trim()) return "AAAA record requires an IP address.";
      if (!IPV6_RE.test(rdata.trim())) return "Invalid IPv6 address.";
      const ip = rdata.trim();
      for (const range of IPV6_PRIVATE) {
        if (range.test(ip)) return "Private IPv6 addresses are not allowed.";
      }
      return null;
    }
    case "CNAME":
    case "NS":
    case "PTR": {
      if (!rdata.trim()) return `${upperType} record requires a target domain.`;
      if (rdata.length > 253) return `${upperType} value too long.`;
      if (!DOMAIN_RE.test(rdata.trim()))
        return `${upperType} record must be a valid domain name.`;
      return null;
    }
    case "TXT": {
      if (rdata.length > 512)
        return `TXT record must not exceed 512 characters (currently ${rdata.length}).`;
      return null;
    }
    case "MX": {
      if (fields.length < 2)
        return "MX record requires: priority hostname";
      const priority = Number(fields[0]);
      if (!Number.isFinite(priority) || !Number.isInteger(priority) || priority < 0 || priority > 65535)
        return "MX priority must be a number (0-65535).";
      if (!DOMAIN_RE.test(fields[1]))
        return "MX hostname must be a valid domain name.";
      return null;
    }
    case "SRV": {
      if (fields.length < 4)
        return "SRV record requires: priority weight port target";
      for (let i = 0; i < 3; i++) {
        const val = Number(fields[i]);
        const label = ["priority", "weight", "port"][i];
        if (!Number.isFinite(val) || !Number.isInteger(val) || val < 0 || val > 65535)
          return `SRV ${label} must be a number (0-65535).`;
      }
      return null;
    }
    default:
      return `Unsupported record type: ${type}`;
  }
}

export function validateReservedTxt(
  type: string,
  name: string,
  rdata: string,
): string | null {
  if (type.toUpperCase() !== "TXT") return null;
  const normalizedName = name === "" ? "@" : name;
  const trimmedRdata = rdata.trimStart();
  if (normalizedName === "_dmarc")
    return "TXT records at _dmarc are reserved (DMARC policy).";
  if (normalizedName.startsWith("_domainkey"))
    return 'TXT records starting with "_domainkey" are reserved (DKIM).';
  if (
    (normalizedName === "@" || normalizedName === "") &&
    trimmedRdata.startsWith("v=spf1")
  )
    return 'TXT records with "v=spf1" at the apex are reserved (SPF).';
  return null;
}

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

export function validateDomainName(name: string): string | null {
  if (!name) return "Name cannot be empty";
  if (name.length > 63) return "Name too long (max 63 characters)";
  if (!/^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/.test(name))
    return "Only lowercase letters, numbers, and hyphens allowed";
  return null;
}

export function validateNsec(nsec: string): string | null {
  if (!nsec.startsWith("nsec1"))
    return "Invalid nsec: must start with 'nsec1'";
  if (nsec.length < 40)
    return "Invalid nsec: too short";
  return null;
}
