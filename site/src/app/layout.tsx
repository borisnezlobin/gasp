import type { Metadata, Viewport } from "next";
import { Charis_SIL } from "next/font/google";
import { SITE_URL } from "@/lib/site";
import "./globals.css";

// Charter is the app's font and ships with every Mac and iPhone; Charis
// SIL is its open-licence cut, fetched only where Charter isn't installed.
const charis = Charis_SIL({
  weight: ["400", "700"],
  style: ["normal", "italic"],
  subsets: ["latin"],
  variable: "--font-charis",
  preload: false,
  display: "swap",
});

const description =
  "A Markdown editor for Mac and iPhone that opens in about 300 ms, stays under 300 MB of memory on huge notes, and syncs for free.";

export const metadata: Metadata = {
  metadataBase: new URL(SITE_URL),
  title: { default: "Gasp, a fast Markdown editor", template: "%s | Gasp" },
  description,
  openGraph: { title: "Gasp", description, url: SITE_URL, siteName: "Gasp", type: "website" },
};

export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#f6f6f7" },
    { media: "(prefers-color-scheme: dark)", color: "#151412" },
  ],
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" className={charis.variable}>
      <body className="min-h-dvh overflow-x-clip">{children}</body>
    </html>
  );
}
