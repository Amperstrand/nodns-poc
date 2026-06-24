import { Badge } from "@/components/ui/badge";

export function SiteHeader() {
  return (
    <header className="border-b border-border bg-card/50 backdrop-blur-sm sticky top-0 z-10">
      <div className="mx-auto max-w-5xl px-4 h-14 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-lg font-bold tracking-tight text-foreground">
            nodns explorer
          </span>
        </div>
        <div className="flex items-center gap-2">
          <Badge className="border-primary/30 text-primary bg-primary/10">
            TESTNET
          </Badge>
          <Badge className="border-border text-muted-foreground bg-muted">
            relay.cashu.email
          </Badge>
        </div>
      </div>
    </header>
  );
}
