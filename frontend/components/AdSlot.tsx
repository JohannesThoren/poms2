"use client";

import { useEffect, useRef } from "react";

declare global {
  interface Window {
    adsbygoogle?: unknown[];
  }
}

/**
 * One AdSense ad unit. Renders nothing unless both
 * NEXT_PUBLIC_ADSENSE_CLIENT_ID and a slot id are configured, so the site
 * looks and behaves exactly as before until real ad IDs are wired in - no
 * placeholder boxes, no broken requests.
 */
export function AdSlot({
  slot,
  format = "auto",
  style,
}: {
  slot: string | undefined;
  format?: string;
  style?: React.CSSProperties;
}) {
  const insRef = useRef<HTMLModElement>(null);
  const pushed = useRef(false);
  const clientId = process.env.NEXT_PUBLIC_ADSENSE_CLIENT_ID;

  useEffect(() => {
    if (!clientId || !slot || pushed.current) return;
    pushed.current = true;
    try {
      (window.adsbygoogle = window.adsbygoogle || []).push({});
    } catch {
      // AdSense script not loaded (blocked, offline, not yet approved) -
      // fail silently, this is a non-essential decoration on the page.
    }
  }, [clientId, slot]);

  if (!clientId || !slot) return null;

  return (
    <ins
      ref={insRef}
      className="adsbygoogle"
      style={style ?? { display: "block" }}
      data-ad-client={clientId}
      data-ad-slot={slot}
      data-ad-format={format}
      data-full-width-responsive="true"
    />
  );
}

const SLOT_ENV: Record<"top" | "left" | "right", string | undefined> = {
  top: process.env.NEXT_PUBLIC_AD_SLOT_TOP,
  left: process.env.NEXT_PUBLIC_AD_SLOT_LEFT,
  right: process.env.NEXT_PUBLIC_AD_SLOT_RIGHT,
};

export function AdBanner({ position }: { position: "top" | "left" | "right" }) {
  return <AdSlot slot={SLOT_ENV[position]} />;
}
