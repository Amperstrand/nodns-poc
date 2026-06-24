import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import type { ZoneStatus } from "@/lib/types";
import { pubkeyToNpub } from "@/lib/nostr";
import { truncateNpub } from "@/lib/format";

interface ZoneCardProps {
  zone: ZoneStatus;
}

export function ZoneCard({ zone }: ZoneCardProps) {
  const npub = pubkeyToNpub(zone.pubkey);

  return (
    <Card className="overflow-hidden">
      <CardContent className="p-4 space-y-3">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <div className="flex items-center gap-2">
            <span className="font-mono text-sm font-semibold text-foreground">
              {zone.zone}
            </span>
            {zone.verified ? (
              <Badge className="border-green-500/30 text-green-400 bg-green-500/10">
                ✓ verified
              </Badge>
            ) : (
              <Badge className="border-destructive/30 text-destructive bg-destructive/10">
                ⚠ unverified
              </Badge>
            )}
            {zone.testnet && (
              <Badge className="border-primary/30 text-primary bg-primary/10">
                TESTNET
              </Badge>
            )}
            <Badge className="border-border text-muted-foreground bg-muted capitalize">
              {zone.status}
            </Badge>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-2 text-xs">
          <div className="space-y-0.5">
            <span className="text-muted-foreground">Operator</span>
            <p className="font-mono text-foreground truncate" title={npub}>
              {truncateNpub(npub)}
            </p>
          </div>
          {zone.pricing && (
            <div className="space-y-0.5">
              <span className="text-muted-foreground">Pricing (sats)</span>
              <p className="font-mono text-foreground">
                create {zone.pricing.create} · update {zone.pricing.update} · delete {zone.pricing.delete}
              </p>
            </div>
          )}
          {zone.mint && (
            <div className="space-y-0.5 col-span-2">
              <span className="text-muted-foreground">Mint</span>
              <p className="font-mono text-foreground truncate" title={zone.mint}>
                {zone.mint}
              </p>
            </div>
          )}
        </div>

        {zone.verificationError && (
          <p className="text-xs text-destructive/80">
            {zone.verificationError}
          </p>
        )}
        {zone.statusReason && !zone.verificationError && (
          <p className="text-xs text-muted-foreground">
            {zone.statusReason}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
