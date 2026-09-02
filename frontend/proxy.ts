import { NextRequest, NextResponse } from "next/server";
import { verifySessionToken, COOKIE_NAME } from "@/lib/session";

// Next.js 16 runs proxy.ts on the Node.js runtime (not Edge), so the same
// HMAC verification used everywhere else works here directly - no need
// for a separate "just check the cookie exists" pass plus a second
// real check in the page.
export function proxy(req: NextRequest) {
  const isLoginRoute = req.nextUrl.pathname.startsWith("/admin/login");
  const isApiRoute = req.nextUrl.pathname.startsWith("/admin/api/");
  if (isLoginRoute || isApiRoute) {
    return NextResponse.next();
  }

  const username = verifySessionToken(req.cookies.get(COOKIE_NAME)?.value);
  if (!username) {
    const url = req.nextUrl.clone();
    url.pathname = "/admin/login";
    return NextResponse.redirect(url);
  }

  return NextResponse.next();
}

export const config = {
  matcher: ["/admin/:path*"],
};
