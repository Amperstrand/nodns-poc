import { SiteHeader } from "@/components/site-header";
import { Hero } from "@/components/hero";
import { Features } from "@/components/features";
import { HowItWorks } from "@/components/how-it-works";
import { RecordBrowserTeaser } from "@/components/record-browser-teaser";
import { LiveFeed } from "@/components/live-feed";
import { FAQ } from "@/components/faq";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { NpubProfile } from "@/components/npub-profile";
import { NpubGate } from "@/components/npub-gate";

export default function Home() {
  return (
    <ErrorBoundary>
      <NpubGate
        profile={<NpubProfile />}
        landing={
          <>
            <SiteHeader />
            <main id="main-content">
              <Hero />
              <Features />
              <HowItWorks />
              <RecordBrowserTeaser />
              <LiveFeed />
              <FAQ />
            </main>
            <SiteFooter />
          </>
        }
      />
    </ErrorBoundary>
  );
}
