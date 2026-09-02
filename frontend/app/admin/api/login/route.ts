import { NextRequest, NextResponse } from "next/server";
import { verifyHostCredentials } from "@/lib/hostAuth";
import { createSessionToken, COOKIE_NAME } from "@/lib/session";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  const body = await req.json().catch(() => null);
  const username = typeof body?.username === "string" ? body.username : "";
  const password = typeof body?.password === "string" ? body.password : "";

  const ok = await verifyHostCredentials(username, password);
  if (!ok) {
    // Deliberately vague - don't reveal whether the username exists or
    // the password was wrong or the group check failed.
    return NextResponse.json({ error: "Fel användarnamn/lösenord, eller inte behörig." }, { status: 401 });
  }

  const token = createSessionToken(username);
  const res = NextResponse.json({ ok: true });
  res.cookies.set(COOKIE_NAME, token, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    maxAge: 8 * 60 * 60,
  });
  return res;
}
