export function timeAgo(timestamp: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - timestamp;

  if (diff < 5) return "just now";
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 2592000) return `${Math.floor(diff / 86400)}d ago`;
  return `${Math.floor(diff / 2592000)}mo ago`;
}

export function truncateNpub(npub: string, head = 12, tail = 8): string {
  if (npub.length <= head + tail + 3) return npub;
  return `${npub.slice(0, head)}...${npub.slice(-tail)}`;
}

export function truncateMid(str: string, maxLen = 20): string {
  if (str.length <= maxLen) return str;
  const half = Math.floor((maxLen - 3) / 2);
  return `${str.slice(0, half)}...${str.slice(-half)}`;
}

export function formatRecordData(type: string, _name: string, data: string): string {
  if (type === "TXT") {
    return `${data}`;
  }
  if (type === "MX") {
    return data;
  }
  return data;
}
