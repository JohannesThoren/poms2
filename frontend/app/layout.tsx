import type { Metadata } from "next";
import { IBM_Plex_Sans, IBM_Plex_Mono } from "next/font/google";
import Script from "next/script";
import "./globals.css";

const plexSans = IBM_Plex_Sans({
  variable: "--font-sans",
  subsets: ["latin"],
  weight: ["400", "500", "600"],
});

const plexMono = IBM_Plex_Mono({
  variable: "--font-mono",
  subsets: ["latin"],
  weight: ["400", "500"],
});

export const metadata: Metadata = {
  title: "POMS2 — Driftläge elnät",
  description: "Aggregerad avbrottsövervakning för svenska nätägare",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  const adsenseClientId = process.env.NEXT_PUBLIC_ADSENSE_CLIENT_ID;
  return (
    <html lang="sv" className={`${plexSans.variable} ${plexMono.variable} h-full`}>
      <body className="min-h-full flex flex-col bg-[var(--bg)] text-[var(--text)]">
        {adsenseClientId && (
          <Script
            async
            src={`https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js?client=${adsenseClientId}`}
            crossOrigin="anonymous"
            strategy="afterInteractive"
          />
        )}
        {children}
      </body>
    </html>
  );
}
