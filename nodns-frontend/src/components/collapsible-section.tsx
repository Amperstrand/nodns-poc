"use client";

import { useState, useEffect } from "react";
import { ChevronDownIcon } from "lucide-react";

interface CollapsibleSectionProps {
  id: string;
  title: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}

export function CollapsibleSection({
  id,
  title,
  defaultOpen = false,
  children,
}: CollapsibleSectionProps) {
  const [open, setOpen] = useState(defaultOpen);

  useEffect(() => {
    const checkHash = () => {
      if (window.location.hash === `#${id}`) {
        setOpen(true);
      }
    };
    checkHash();
    window.addEventListener("hashchange", checkHash);
    return () => window.removeEventListener("hashchange", checkHash);
  }, [id]);

  return (
    <div id={id} className="scroll-mt-32 border-b border-border">
      <button
        onClick={() => setOpen(!open)}
        className="flex w-full items-center justify-between px-6 py-4 text-left hover:bg-muted/20 transition-colors"
        aria-expanded={open}
      >
        <h2 className="text-[1.75rem] font-bold tracking-tight">{title}</h2>
        <span className={`flex size-8 items-center justify-center rounded-full bg-muted/40 transition-transform duration-200 ${open ? "rotate-180" : ""}`}>
          <ChevronDownIcon className="size-5 text-foreground" />
        </span>
      </button>
      <div
        className={`grid transition-all duration-300 ease-in-out ${
          open ? "grid-rows-[1fr]" : "grid-rows-[0fr]"
        }`}
      >
        <div className="overflow-hidden">{children}</div>
      </div>
    </div>
  );
}
