"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";

export default function LoginPage() {
  const router = useRouter();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/admin/api/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ username, password }),
      });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        setError(data.error || "Inloggning misslyckades.");
        setLoading(false);
        return;
      }
      router.push("/admin");
      router.refresh();
    } catch {
      setError("Kunde inte nå servern.");
      setLoading(false);
    }
  }

  return (
    <main className="flex-1 flex flex-col items-center justify-center min-h-screen bg-[var(--bg)]">
      <form
        onSubmit={handleSubmit}
        className="w-full max-w-[320px] border border-[var(--line)] p-6"
      >
        <h1 className="text-[15px] font-medium text-[var(--text)] mb-1">POMS2 admin</h1>
        <p className="text-sm text-[var(--muted)] mb-6">Logga in med ditt konto på servern</p>

        <label className="block text-sm text-[var(--muted)] mb-1" htmlFor="username">
          Användarnamn
        </label>
        <input
          id="username"
          type="text"
          autoComplete="username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          className="w-full mb-4 px-3 py-2 bg-[var(--panel)] border border-[var(--line)] text-[var(--text)] text-sm focus:outline-none focus:border-[var(--upcoming)]"
        />

        <label className="block text-sm text-[var(--muted)] mb-1" htmlFor="password">
          Lösenord
        </label>
        <input
          id="password"
          type="password"
          autoComplete="current-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className="w-full mb-4 px-3 py-2 bg-[var(--panel)] border border-[var(--line)] text-[var(--text)] text-sm focus:outline-none focus:border-[var(--upcoming)]"
        />

        {error && <p className="text-sm mb-4" style={{ color: "var(--fault)" }}>{error}</p>}

        <button
          type="submit"
          disabled={loading}
          className="w-full py-2 text-sm border border-[var(--line)] text-[var(--text)] hover:bg-[var(--panel)] transition-colors disabled:opacity-50"
        >
          {loading ? "Loggar in…" : "Logga in"}
        </button>
      </form>
    </main>
  );
}
