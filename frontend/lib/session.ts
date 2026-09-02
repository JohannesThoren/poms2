import { createHmac, timingSafeEqual } from "crypto";

const COOKIE_NAME = "poms_admin_session";
const SESSION_TTL_MS = 8 * 60 * 60 * 1000; // 8 hours

function secret(): string {
  const s = process.env.ADMIN_SESSION_SECRET;
  if (!s) {
    throw new Error(
      "ADMIN_SESSION_SECRET must be set to a random value - the admin login cannot run without it"
    );
  }
  return s;
}

function sign(payload: string): string {
  return createHmac("sha256", secret()).update(payload).digest("hex");
}

/** Builds the cookie value: `username.expiryMs.hmac`. */
export function createSessionToken(username: string): string {
  const expiry = Date.now() + SESSION_TTL_MS;
  const payload = `${username}.${expiry}`;
  return `${payload}.${sign(payload)}`;
}

/** Returns the authenticated username if the token is valid and unexpired. */
export function verifySessionToken(token: string | undefined): string | null {
  if (!token) return null;
  const parts = token.split(".");
  if (parts.length !== 3) return null;
  const [username, expiryStr, mac] = parts;
  const payload = `${username}.${expiryStr}`;
  const expected = sign(payload);

  const a = Buffer.from(mac);
  const b = Buffer.from(expected);
  if (a.length !== b.length || !timingSafeEqual(a, b)) return null;

  const expiry = Number(expiryStr);
  if (!Number.isFinite(expiry) || expiry < Date.now()) return null;

  return username;
}

export { COOKIE_NAME };
