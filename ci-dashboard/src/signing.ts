import { X509Certificate } from "node:crypto";

/**
 * Days left under which an item is flagged, in order: a warning under 30 and
 * under 14, and under the last the run fails, leaving a week to renew before
 * a release breaks. The dashboard's server posts the failure to Matrix, as
 * for any watched workflow.
 */
export const EXPIRY_THRESHOLDS = [30, 14, 7];

const DAY_MS = 86_400_000;

export type SigningItem = {
  /** Stable while the item is unchanged: a profile UUID or a fingerprint. */
  id: string;
  kind: "certificate" | "profile";
  name: string;
  expires: Date;
};

/**
 * Name, UUID and dates from a .mobileprovision. The file is a signed CMS
 * envelope around a plain XML plist, so the plist is read straight out of it
 * and no macOS tool is needed.
 */
export type Profile = SigningItem & {
  created: Date;
  /** The bundle identifier, without the team prefix. */
  appId: string;
};

export function parseProfile(bytes: Uint8Array): Profile | undefined {
  const text = new TextDecoder("latin1").decode(bytes);
  const plist = /<\?xml[\s\S]*?<\/plist>/.exec(text)?.[0];
  if (!plist) {
    return undefined;
  }
  const value = (key: string, type: "string" | "date") =>
    new RegExp(`<key>${key}</key>\\s*<${type}>([^<]*)</${type}>`).exec(
      plist,
    )?.[1];
  const name = value("Name", "string");
  const id = value("UUID", "string");
  const expires = value("ExpirationDate", "date");
  const created = value("CreationDate", "date");
  const appId = value("application-identifier", "string")?.replace(
    /^[^.]+\./,
    "",
  );
  if (!name || !id || !expires || !created || !appId) {
    return undefined;
  }
  return {
    id,
    kind: "profile",
    name,
    expires: new Date(expires),
    created: new Date(created),
    appId,
  };
}

/**
 * The profiles match installed. On a shared Mac the directory also holds
 * other jobs' profiles and older copies of these, so only match's own names
 * for the given apps are kept, and for each name only the newest. With no
 * apps given, every match profile is kept.
 */
export function currentProfiles(
  profiles: Profile[],
  appIds: string[] = [],
): SigningItem[] {
  const newest = new Map<string, Profile>();
  for (const profile of profiles) {
    if (
      !profile.name.startsWith("match ") ||
      (appIds.length > 0 && !appIds.includes(profile.appId))
    ) {
      continue;
    }
    const seen = newest.get(profile.name);
    if (!seen || profile.created > seen.created) {
      newest.set(profile.name, profile);
    }
  }
  return [...newest.values()].map(({ created: _, appId: __, ...item }) => item);
}

/** The signing identities in a PEM bundle, leaving out Apple's CA certificates. */
export function parseCertificates(pem: string): SigningItem[] {
  const blocks =
    pem.match(
      /-----BEGIN CERTIFICATE-----[\s\S]+?-----END CERTIFICATE-----/g,
    ) ?? [];
  const items = new Map<string, SigningItem>();
  for (const block of blocks) {
    const certificate = new X509Certificate(block);
    const name = /^CN=(.*)$/m.exec(certificate.subject)?.[1] ?? "";
    if (!/^(Apple|iPhone) (Distribution|Development|Developer)\b/.test(name)) {
      continue;
    }
    items.set(certificate.fingerprint256, {
      id: certificate.fingerprint256,
      kind: "certificate",
      name,
      expires: new Date(certificate.validTo),
    });
  }
  return [...items.values()];
}

export function daysLeft(item: SigningItem, now: Date): number {
  return Math.floor((item.expires.getTime() - now.getTime()) / DAY_MS);
}

/** How many thresholds the item is under: 0 while it has 30 days or more. */
export function stage(days: number): number {
  return EXPIRY_THRESHOLDS.filter((threshold) => days < threshold).length;
}

function describe(item: SigningItem, now: Date): string {
  const days = daysLeft(item, now);
  const date = item.expires.toISOString().slice(0, 10);
  if (days < 0) return `expired on ${date}`;
  if (days === 0) return `expires today (${date})`;
  return `expires in ${days} ${days === 1 ? "day" : "days"} (${date})`;
}

const ICONS = ["✅", "⚠️", "🚨", "❌"];

export function summary(items: SigningItem[], now: Date): string {
  const sorted = [...items].sort(
    (a, b) => a.expires.getTime() - b.expires.getTime(),
  );
  return [
    "### iOS certificates and profiles",
    "",
    "| | Kind | Name | Expiry |",
    "| --- | --- | --- | --- |",
    ...sorted.map(
      (item) =>
        `| ${ICONS[stage(daysLeft(item, now))]} | ${item.kind} | ${item.name} | ${describe(item, now)} |`,
    ),
    "",
  ].join("\n");
}
