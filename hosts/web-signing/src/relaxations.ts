/** Protections a developer can turn off for one tab, each on purpose. */
export interface Relaxations {
  /** Serve a product archive without the container, so its requests are not gated. */
  archiveWithoutContainer: boolean;
  /** Answer a product's network permission prompts with a one-time yes, unasked. */
  approveNetworkWithoutAsking: boolean;
}

export const NO_RELAXATIONS: Relaxations = {
  archiveWithoutContainer: false,
  approveNetworkWithoutAsking: false,
};

const KEY = "truapi-web-signing-host:relaxations";

/** The relaxations saved for this tab. Anything unreadable means none. */
export function loadRelaxations(
  storage: Pick<Storage, "getItem">,
): Relaxations {
  try {
    const saved: unknown = JSON.parse(storage.getItem(KEY) ?? "null");
    if (typeof saved !== "object" || saved === null) return NO_RELAXATIONS;
    const record = saved as Record<string, unknown>;
    return {
      archiveWithoutContainer: record.archiveWithoutContainer === true,
      approveNetworkWithoutAsking: record.approveNetworkWithoutAsking === true,
    };
  } catch {
    return NO_RELAXATIONS;
  }
}

export function saveRelaxations(
  storage: Pick<Storage, "setItem">,
  relaxations: Relaxations,
): void {
  storage.setItem(KEY, JSON.stringify(relaxations));
}

/** What each relaxation on gives up, in words for a banner. */
export function activeRelaxations(relaxations: Relaxations): string[] {
  const lines: string[] = [];
  if (relaxations.archiveWithoutContainer)
    lines.push("archives run without the container (requests not gated)");
  if (relaxations.approveNetworkWithoutAsking)
    lines.push("network approved without asking");
  return lines;
}

export function sameRelaxations(a: Relaxations, b: Relaxations): boolean {
  return (
    a.archiveWithoutContainer === b.archiveWithoutContainer &&
    a.approveNetworkWithoutAsking === b.approveNetworkWithoutAsking
  );
}
