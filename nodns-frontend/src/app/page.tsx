import { SiteHeader } from "@/components/site-header";
import { Hero } from "@/components/hero";
import { Features } from "@/components/features";
import { HowItWorks } from "@/components/how-it-works";
import { RecordBrowserTeaser } from "@/components/record-browser-teaser";
import { LiveFeed } from "@/components/live-feed";
import { PublishDemo } from "@/components/publish-demo";
import { FAQ } from "@/components/faq";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";

export default function Home() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main>
        <Hero />
        <Features />
        <HowItWorks />
        <RecordBrowserTeaser />
        <LiveFeed />
        <PublishDemo />
        <FAQ />
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
