"use client";

import { useState, useEffect, type ReactNode } from "react";

function isNpubSubdomain(hostname: string): boolean {
  if (!hostname.endsWith(".nodns.shop")) return false;
  const sub = hostname.replace(".nodns.shop", "");
  return sub.startsWith("npub1") && sub.length > 10;
}

export function NpubGate({
  profile,
  landing,
}: {
  profile: ReactNode;
  landing: ReactNode;
}) {
  const [isNpub, setIsNpub] = useState<boolean | null>(null);

  useEffect(() => {
    setIsNpub(isNpubSubdomain(window.location.hostname));
  }, []);

  if (isNpub === null) {
    return null;
  }

  return isNpub ? <>{profile}</> : <>{landing}</>;
}
