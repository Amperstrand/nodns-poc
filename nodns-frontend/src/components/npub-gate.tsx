"use client";

import { useSyncExternalStore, type ReactNode } from "react";

function isNpubSubdomain(hostname: string): boolean {
  if (!hostname.endsWith(".nodns.shop")) return false;
  const sub = hostname.replace(".nodns.shop", "");
  return sub.startsWith("npub1") && sub.length > 10;
}

function subscribe() {
  return () => {};
}

function getSnapshot() {
  return isNpubSubdomain(window.location.hostname);
}

function getServerSnapshot() {
  return false;
}

export function NpubGate({
  profile,
  landing,
}: {
  profile: ReactNode;
  landing: ReactNode;
}) {
  const isNpub = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);

  return isNpub ? <>{profile}</> : <>{landing}</>;
}
