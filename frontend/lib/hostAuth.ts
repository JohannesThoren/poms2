import { execFile } from "child_process";
import { promisify } from "util";

// authenticate-pam has no types; wrap it loosely.
// eslint-disable-next-line @typescript-eslint/no-var-requires
const pam = require("authenticate-pam");

const execFileAsync = promisify(execFile);

/** Which host group a user must belong to in order to use the admin page. */
const ADMIN_GROUP = process.env.ADMIN_GROUP || "poms-admin";

function pamAuthenticate(username: string, password: string): Promise<void> {
  return new Promise((resolve, reject) => {
    pam.authenticate(username, password, (err: unknown) => {
      if (err) reject(err instanceof Error ? err : new Error(String(err)));
      else resolve();
    });
  });
}

async function isInAdminGroup(username: string): Promise<boolean> {
  try {
    const { stdout } = await execFileAsync("id", ["-nG", username]);
    return stdout.trim().split(/\s+/).includes(ADMIN_GROUP);
  } catch {
    return false;
  }
}

/**
 * Verifies a username/password against the host's PAM stack (the same
 * users the machine itself logs in with - no separate app user database)
 * and requires membership in the `poms-admin` host group.
 *
 * Note for deployment: PAM's `pam_unix` module needs to read /etc/shadow,
 * so the process running this (the frontend container) needs the
 * `shadow` group or equivalent - see the frontend Dockerfile.
 */
export async function verifyHostCredentials(username: string, password: string): Promise<boolean> {
  if (!username || !password) return false;
  try {
    await pamAuthenticate(username, password);
  } catch {
    return false;
  }
  return isInAdminGroup(username);
}
