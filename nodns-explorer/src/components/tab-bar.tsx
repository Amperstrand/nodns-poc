import { cn } from "@/lib/utils";

export type MonitorTab = "feed" | "monitor";

interface TabBarProps {
  tab: MonitorTab;
  setTab: (tab: MonitorTab) => void;
}

const TABS: { id: MonitorTab; label: string }[] = [
  { id: "feed", label: "Event Feed" },
  { id: "monitor", label: "Zone Monitor" },
];

export function TabBar({ tab, setTab }: TabBarProps) {
  return (
    <div className="border-b border-border bg-card/30">
      <div className="mx-auto max-w-5xl px-4 flex gap-1">
        {TABS.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={cn(
              "px-4 py-3 text-sm font-medium transition-colors border-b-2 -mb-px",
              tab === t.id
                ? "border-primary text-primary"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            {t.label}
          </button>
        ))}
      </div>
    </div>
  );
}
