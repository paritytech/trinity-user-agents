import { execSync } from "node:child_process";
import worker from "./worker-manifest.json";

/**
 * Product manifest for the fundtest reference funding provider.
 *
 * `domain` must equal the name `bulletin-deploy` is invoked with, byte for
 * byte; on Paseo Next v2 that is `fundtest.paseo`. The App is the provider's
 * screens (`/card`, `/bank`, `/payout`), the Worker serves FundingProvider and
 * declares its routes under `includes.funding`, read from
 * worker-manifest.json so the snapshot a host ships and the published record
 * cannot drift apart.
 */

const buildId = (() => {
  try {
    return execSync("git rev-parse --short HEAD", { encoding: "utf8" }).trim();
  } catch {
    return "dev";
  }
})();

const [major, minor, patch] = worker.appVersion;
const appVersion = [major, minor, patch, buildId];

export default {
  domain: process.env.FUNDTEST_DOTNS_NAME ?? "fundtest.paseo",
  displayName: "Fundtest",
  description:
    "Reference funding provider for Paseo: a simulated fiat leg in front of the real route to CASH.",
  icon: { path: "icon.png", format: "png" },
  executables: [
    { kind: "app", path: "dist/app", appVersion },
    {
      kind: "worker",
      path: "dist/worker",
      appVersion,
      entrypoint: worker.entrypoint,
      includes: { chat: false, pocket: false, ...worker.includes },
    },
  ],
};
