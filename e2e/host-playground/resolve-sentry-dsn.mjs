import { resolveAndroidDsn } from "./verify-sentry-ingestion.mjs";
import { readFileSync } from "node:fs";

const destination = process.env.SENTRY_DSN_FILE;
resolveAndroidDsn({
  apiBase: process.env.SENTRY_API_BASE,
  org: process.env.SENTRY_ORG,
  nativeProject: process.env.SENTRY_NATIVE_PROJECT,
  jollityProject: process.env.SENTRY_JOLLITY_PROJECT,
  token: process.env.SENTRY_AUTH_TOKEN,
  overrideDsn: process.env.SENTRY_DSN_ANDROID,
  destination,
}).then(({ project, resolvedFromApi }) => {
  const dsn = readFileSync(destination, "utf8");
  process.stdout.write(`::add-mask::${dsn}\n`);
  console.log(`Resolved Android Sentry DSN for ${project} (${resolvedFromApi ? "Sentry API" : "configured secret"})`);
}).catch((error) => {
  console.error(`[sentry preflight] ${error.message}`);
  process.exitCode = 1;
});
