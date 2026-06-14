import { Suspense } from "react";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { Discoveries } from "@/components/discoveries";

function DiscoveriesContent() {
  return (
    <div className="mx-auto max-w-[960px] py-8 md:py-12">
      <Discoveries />
    </div>
  );
}

export default function DiscoveriesPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main id="main-content" className="px-6 pb-16">
        <Suspense
          fallback={
            <div className="mx-auto max-w-[960px] py-20 text-center text-muted-foreground animate-pulse">
              Loading...
            </div>
          }
        >
          <DiscoveriesContent />
        </Suspense>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
