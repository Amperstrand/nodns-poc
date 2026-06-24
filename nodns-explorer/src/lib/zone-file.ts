export interface ZoneRecord {
  name: string;
  type: string;
  ttl: number;
  rdata: string;
  npub: string;
  event_id: string;
  created_at: number;
}

function toRelativeLabel(fqdn: string, zone: string): string {
  const suffix = `.${zone}`.toLowerCase();
  const lower = fqdn.toLowerCase();
  if (lower.endsWith(suffix)) {
    const rel = fqdn.slice(0, fqdn.length - suffix.length);
    return rel.length > 0 ? rel : "@";
  }
  return fqdn;
}

function escapeTxt(data: string): string {
  return `"${data.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

export function generateZoneFile(records: ZoneRecord[], zone: string): string {
  const sorted = [...records].sort((a, b) => {
    const nameCmp = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
    if (nameCmp !== 0) return nameCmp;
    const typeCmp = a.type.toUpperCase().localeCompare(b.type.toUpperCase());
    if (typeCmp !== 0) return typeCmp;
    return a.rdata.localeCompare(b.rdata);
  });

  const labels = sorted.map((r) => toRelativeLabel(r.name, zone));
  const nameWidth = Math.max("@".length, ...labels.map((l) => l.length));
  const typeWidth = Math.max("IN".length + 1, ...sorted.map((r) => r.type.length));

  const lines: string[] = [
    `$ORIGIN ${zone}.`,
    `$TTL 3600`,
    `; generated ${new Date().toISOString()}`,
    `; ${records.length} records`,
    "",
  ];

  for (let i = 0; i < sorted.length; i++) {
    const r = sorted[i];
    const label = labels[i] || "@";
    const rdata = r.type.toUpperCase() === "TXT" ? escapeTxt(r.rdata) : r.rdata;
    lines.push(
      `${label.padEnd(nameWidth)}  IN  ${r.type.toUpperCase().padEnd(typeWidth)}  ${r.ttl}  ${rdata}`,
    );
  }

  return lines.join("\n") + "\n";
}

export function downloadZoneFile(content: string, filename: string): void {
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
