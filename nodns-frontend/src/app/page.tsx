import { SiteHeader } from "@/components/site-header";
import { Hero } from "@/components/hero";
import { Features } from "@/components/features";
import { HowItWorks } from "@/components/how-it-works";
import { RecordBrowser } from "@/components/record-browser";
import { LiveFeed } from "@/components/live-feed";
import { Dashboard } from "@/components/dashboard";
import { TryIt } from "@/components/try-it";
import { Architecture } from "@/components/architecture";
import { ProtocolSpec } from "@/components/protocol-spec";
import { FAQ } from "@/components/faq";
import { Roadmap } from "@/components/roadmap";
import { Infrastructure } from "@/components/infrastructure";
import { SiteFooter } from "@/components/site-footer";

export default function Home() {
  return (
    <>
      <SiteHeader />
      <main>
        <Hero />
        <Features />
        <HowItWorks />
        <RecordBrowser />
        <LiveFeed />
        <Dashboard />
        <TryIt />
        <Architecture />
        <ProtocolSpec />
        <FAQ />
        <Roadmap />
        <Infrastructure />
      </main>
      <SiteFooter />
    </>
  );
}
