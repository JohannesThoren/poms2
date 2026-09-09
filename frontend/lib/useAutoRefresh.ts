"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";

/**
 * Polls the server for fresh data by calling `router.refresh()` on an
 * interval. This re-runs the page's server components (re-querying the
 * DB) and patches in the new data without a full navigation - local
 * client state in the calling component (filters, sort order, the
 * selected outage, scroll position) survives untouched, only the
 * server-fetched props change.
 *
 * Pauses while the tab is hidden (no point re-fetching a dashboard
 * nobody's looking at) and refreshes once immediately on becoming
 * visible again, so switching back to the tab always shows current data
 * rather than whatever was last fetched before it was hidden.
 *
 * Returns the Date of the last refresh, so callers can show a
 * "senast uppdaterad" indicator that's guaranteed to reflect what's
 * actually on screen.
 */
export function useAutoRefresh(intervalMs: number): Date {
  const router = useRouter();
  const [lastRefreshed, setLastRefreshed] = useState(() => new Date());
  const routerRef = useRef(router);
  useEffect(() => {
    routerRef.current = router;
  }, [router]);

  useEffect(() => {
    function refresh() {
      routerRef.current.refresh();
      setLastRefreshed(new Date());
    }

    function tick() {
      if (document.visibilityState === "visible") {
        refresh();
      }
    }

    const id = setInterval(tick, intervalMs);

    function onVisibilityChange() {
      if (document.visibilityState === "visible") {
        refresh();
      }
    }
    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      clearInterval(id);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, [intervalMs]);

  return lastRefreshed;
}
