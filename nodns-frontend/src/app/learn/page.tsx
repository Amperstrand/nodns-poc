import { Suspense } from "react";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { Architecture } from "@/components/architecture";
import { Consensus } from "@/components/consensus";
import { ProtocolSpec } from "@/components/protocol-spec";
import { Roadmap } from "@/components/roadmap";

function LearnContent() {
  return (
    <div className="mx-auto max-w-[960px] py-8 md:py-12">
      <Architecture />
      <Consensus />
      <ProtocolSpec />
      <Roadmap />
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
