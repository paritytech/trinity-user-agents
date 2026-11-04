import type { DockAddresses } from "./shell.js";
import { normalizeNetwork } from "../platform/network-scope.js";

/** What a destination opens before the user chooses. Only names known to be published are listed. */
const DEFAULTS: Record<string, Partial<DockAddresses>> = {
  paseo: { browse: "browse.paseo" },
};

/**
 * The addresses the dock opens on `network` before the user chooses. Kept
 * apart from the shell, so the build can read the Browse name without loading
 * the host.
 */
export function defaultAddresses(network: string): DockAddresses {
  return {
    browse: "",
    ...DEFAULTS[normalizeNetwork(network)],
  };
}
