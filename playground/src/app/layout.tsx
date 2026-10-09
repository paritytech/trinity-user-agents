import type { Metadata, Viewport } from "next";
import localFont from "next/font/local";
import "./globals.css";
import packageJson from "../../package.json";

const serif = localFont({
  src: [
    {
      path: "./fonts/InstrumentSerif-Regular.ttf",
      weight: "400",
      style: "normal",
    },
    {
      path: "./fonts/InstrumentSerif-Italic.ttf",
      weight: "400",
      style: "italic",
    },
  ],
  adjustFontFallback: "Times New Roman",
  variable: "--font-serif",
  display: "swap",
});

const sans = localFont({
  src: "./fonts/Geist.ttf",
  weight: "400 700",
  style: "normal",
  variable: "--font-sans",
  display: "swap",
});

const mono = localFont({
  src: "./fonts/JetBrainsMono.ttf",
  weight: "400 600",
  style: "normal",
  variable: "--font-mono",
  display: "swap",
});

export const metadata: Metadata = {
  title: `TrUAPI Playground v${packageJson.version}`,
  description: "Interactive playground for testing the TrUAPI API",
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  themeColor: "#F4EFE4",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html
      lang="en"
      className={`${serif.variable} ${sans.variable} ${mono.variable}`}
    >
      <body>
        {process.env.NODE_ENV === "development" && (
          // Blocking on purpose: the bridge installs window.__HOST_API_PORT__,
          // which product code reads synchronously. Stripped from production
          // builds, so the page-weight rule this waives does not apply.
          // eslint-disable-next-line @next/next/no-sync-scripts
          <script src="http://127.0.0.1:9955/bootstrap.js" />
        )}
        {children}
      </body>
    </html>
  );
}
