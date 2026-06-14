import { Suspense } from "react";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { Architecture } from "@/components/architecture";
import { Consensus } from "@/components/consensus";
import { ProtocolSpec } from "@/components/protocol-spec";
import { Roadmap } from "@/components/roadmap";

const SECTIONS = [
  { id: "architecture", label: "Architecture" },
  { id: "consensus", label: "Consensus" },
  { id: "protocol", label: "Protocol" },
  { id: "roadmap", label: "Roadmap" },
];

function LearnContent() {
  return (
    <div id="top" className="mx-auto max-w-[960px] py-8 md:py-12">
      <nav className="sticky top-16 z-30 -mx-6 mb-4 border-b border-border bg-background/95 px-6 py-2.5 backdrop-blur">
        <div className="flex items-center gap-1 overflow-x-auto">
          {SECTIONS.map((s) => (
            <a
              key={s.id}
              href={`#${s.id}`}
              className="shrink-0 rounded-md px-3 py-1.5 text-sm font-medium text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
            >
              {s.label}
            </a>
          ))}
        </div>
      </nav>
      <Architecture />
      <Consensus />
      <ProtocolSpec />
      <Roadmap />
      <div className="mt-12 flex justify-center">
        <a
          href="#top"
          className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-card px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground hover:border-primary/30 transition-colors min-h-[44px]"
        >
          <svg className="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="m18 15-6-6-6 6" />
          </svg>
          Back to top
        </a>
      </div>
    </div>
  );
}

export default function LearnPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main className="px-6 pb-16">
        <Suspense
          fallback={
            <div className="mx-auto max-w-[960px] py-20 text-center text-muted-foreground animate-pulse">
              Loading...
            </div>
          }
        >
          <LearnContent />
        </Suspense>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
