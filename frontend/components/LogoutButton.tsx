"use client";

import { useRouter } from "next/navigation";

export function LogoutButton() {
  const router = useRouter();

  async function handleLogout() {
    await fetch("/admin/api/logout", { method: "POST" });
    router.push("/admin/login");
    router.refresh();
  }

  return (
    <button
      onClick={handleLogout}
      className="px-3 py-1.5 text-sm border border-[var(--line)] text-[var(--muted)] hover:text-[var(--text)] hover:bg-[var(--panel)] transition-colors"
    >
      Logga ut
    </button>
  );
}
