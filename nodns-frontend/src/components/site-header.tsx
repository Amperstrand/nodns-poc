"use client";

import Link from "next/link";

const NAV_ITEMS = [
  { href: "#what", label: "What is NoDNS" },
  { href: "#records", label: "Records" },
  { href: "#dashboard", label: "Dashboard" },
  { href: "#protocol", label: "Protocol" },
  { href: "#faq", label: "FAQ" },
];

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-50 border-b border-[#222] bg-[rgba(10,10,10,0.95)] backdrop-blur-[12px]">
      <div className="mx-auto flex max-w-[960px] items-center justify-between gap-3 px-6 py-5">
        <Link href="/" className="text-xl font-bold tracking-tight">
          No<span className="text-[#ff6b35]">DNS</span>.shop
        </Link>
        <nav className="hidden items-center gap-2 sm:flex">
          {NAV_ITEMS.map((item) => (
            <a
              key={item.href}
              href={item.href}
              className="rounded-md px-3 py-1.5 text-sm text-[#666] transition-colors hover:bg-[#222] hover:text-[#e0e0e0]"
            >
              {item.label}
            </a>
          ))}
        </nav>
      </div>
    </header>
  );
}
