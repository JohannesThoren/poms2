import { NextResponse } from "next/server";

// AdSense requires ads.txt at the domain root once you have a real
// publisher id, to confirm you're an authorized seller for your own
// inventory. Generated from the same env var as the ad units themselves -
// nothing to keep in sync by hand. Empty (200, blank body) until
// NEXT_PUBLIC_ADSENSE_CLIENT_ID is set.
export async function GET() {
  const clientId = process.env.NEXT_PUBLIC_ADSENSE_CLIENT_ID;
  const pubId = clientId?.replace(/^ca-/, "");
  const body = pubId ? `google.com, ${pubId}, DIRECT, f08c47fec0942fa0\n` : "";

  return new NextResponse(body, {
    headers: { "Content-Type": "text/plain" },
  });
}
