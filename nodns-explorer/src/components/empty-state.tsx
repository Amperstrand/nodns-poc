import { Card } from "@/components/ui/card";

export function LoadingState() {
  return (
    <Card className="divide-y divide-border">
      {[...Array(5)].map((_, i) => (
        <div key={i} className="px-4 py-3 flex items-center gap-3 animate-pulse">
          <div className="h-5 w-16 rounded-full bg-muted" />
          <div className="flex-1 space-y-2">
            <div className="h-3 w-24 rounded bg-muted" />
            <div className="h-3 w-48 rounded bg-muted" />
          </div>
          <div className="h-3 w-10 rounded bg-muted" />
        </div>
      ))}
    </Card>
  );
}

export function EmptyState({ message }: { message: string }) {
  return (
    <Card className="p-12 flex flex-col items-center justify-center text-center">
      <p className="text-sm text-muted-foreground">{message}</p>
    </Card>
  );
}

export function ZoneLoadingState() {
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      {[...Array(2)].map((_, i) => (
        <Card key={i} className="p-4 space-y-3 animate-pulse">
          <div className="flex gap-2">
            <div className="h-5 w-32 rounded-full bg-muted" />
            <div className="h-5 w-16 rounded-full bg-muted" />
          </div>
          <div className="space-y-1.5">
            <div className="h-3 w-20 rounded bg-muted" />
            <div className="h-3 w-40 rounded bg-muted" />
          </div>
        </Card>
      ))}
    </div>
  );
}
