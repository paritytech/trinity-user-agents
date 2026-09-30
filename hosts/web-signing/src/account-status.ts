import type { AuthState } from "@parity/truapi-host";
import type { AllocationRecord } from "./allowance-ledger.js";
import type {
  AccountReading,
  PeopleChainReading,
  RingMembership,
} from "./people-chain.js";
import { resourceRows, type ResourcesState } from "./product-resources.js";
import type { VersionRow } from "./versions.js";

/** What the status reads of the host: the network it runs on and the wallet imported into the tab. */
export interface AccountContext {
  networkSuffix: string;
  walletName: string | null;
}

/** Where the People chain read for the signed-in account stands. */
export type ChainState =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "error"; message: string }
  | { state: "done"; checkedAt: Date; reading: PeopleChainReading };

/** `0x625e25…3b715`: enough to tell keys apart; the full key is the tooltip. */
export function abbreviate(hex: string): string {
  return hex.length > 20 ? `${hex.slice(0, 8)}…${hex.slice(-5)}` : hex;
}

const SOURCE =
  "People chain storage (Resources.Consumers, PeopleLite.LitePeople), read through this host's light client at the finalized block. Layouts follow the committed People chain metadata.";

const ROLE_LABEL = { identity: "Identity account", root: "Root key" } as const;

function describeReading(reading: AccountReading): VersionRow {
  const label = `On chain: ${ROLE_LABEL[reading.role].toLowerCase()}`;
  if (reading.problem !== undefined)
    return {
      label,
      value: "Unreadable",
      state: "warning",
      title: reading.problem,
    };
  const { consumer } = reading;
  if (consumer === null) {
    return reading.litePerson
      ? {
          label,
          value: "Lite person",
          detail: "no username record",
          title:
            "PeopleLite.LitePeople has an entry. Resources.Consumers has none.",
        }
      : {
          label,
          value: "Not found",
          title: `No Resources.Consumers or PeopleLite.LitePeople entry for this ${ROLE_LABEL[reading.role].toLowerCase()}. The person may be registered under a different account.`,
        };
  }
  const { credibility } = consumer;
  const standing =
    credibility.tag === "Lite"
      ? "Lite"
      : credibility.demoted
        ? "Person (demoted)"
        : "Person";
  const name = consumer.fullUsername ?? consumer.liteUsername;
  const updated =
    credibility.tag === "Person"
      ? `\nCredibility last updated ${new Date(Number(credibility.lastUpdate) * 1000).toISOString()}. Demotion is set only when someone submits it, so not demoted does not mean current.`
      : "";
  return {
    label,
    value: `${standing} · ${name}`,
    detail: consumer.fullUsername ? `lite ${consumer.liteUsername}` : undefined,
    title: `${reading.accountId}${updated}`,
  };
}

function slotsRow(readings: AccountReading[]): VersionRow {
  const label = "Consumer slots";
  const holder = readings.find((reading) => reading.consumer !== null);
  if (holder?.consumer == null)
    return {
      label,
      value: "No record",
      title:
        "Slots are part of a Resources.Consumers record, and none was found.",
    };
  const { slots } = holder.consumer;
  const occupied = slots.flatMap((slot) =>
    slot.tag === "Occupied" ? [slot.accountId] : [],
  );
  return {
    label,
    value: `${occupied.length} of ${slots.length} occupied`,
    detail: ROLE_LABEL[holder.role].toLowerCase(),
    title: `Resources.Consumers.stmt_store_slots of this record, as stored. The pallet does not document them. They are not product allowances: the core registers those in Resources.StatementStoreAllowances, by ring alias, and never writes this record. This is not remaining quota.${occupied.length > 0 ? `\nOccupied by:\n${occupied.join("\n")}` : ""}`,
  };
}

const RING_NOTE =
  "Members.Members[(collection, ring key)] at the finalized block. The ring key is the one this account's own PeopleLite.LitePeople, or People.AccountToPersonalId and People.People, record names. It is not derived from the recovery phrase here, so this does not prove the key the wallet signs with is the listed one. Diagnostic only: it does not by itself explain a signing failure. A runtime that moved these items reads as no key, not as an error.";

const RING_LABEL = { LitePeople: "lite", People: "full" } as const;

function ringRow(
  role: AccountReading["role"],
  { collection, lookup }: RingMembership,
): VersionRow | null {
  const label = `Ring: ${RING_LABEL[collection]}`;
  const detail = ROLE_LABEL[role].toLowerCase();
  switch (lookup.tag) {
    case "NoKey":
      return null;
    case "Unreadable":
      return {
        label,
        value: "Unreadable",
        detail,
        state: "warning",
        title: lookup.reason,
      };
    case "NotListed":
      return {
        label,
        value: "No Members entry",
        detail,
        title: `Ring key ${lookup.key}\n${RING_NOTE}`,
      };
    case "Listed": {
      const { position } = lookup;
      const title = `Ring key ${lookup.key}\n${RING_NOTE}`;
      if (position.tag === "Included")
        return {
          label,
          value: `Included · ring ${position.ringIndex}`,
          detail: `${detail}, position ${position.ringPosition}`,
          title,
        };
      if (position.tag === "Onboarding")
        return {
          label,
          value: "Onboarding",
          detail,
          title: `Queue page ${position.queuePage}, queued at ${position.queuedAt}.\n${title}`,
        };
      return { label, value: "Suspended", detail, state: "warning", title };
    }
  }
}

function ringRows(readings: AccountReading[]): VersionRow[] {
  const rows = readings.flatMap((reading) =>
    reading.rings.flatMap((ring) => ringRow(reading.role, ring) ?? []),
  );
  return rows.length > 0
    ? rows
    : [
        {
          label: "Ring",
          value: "No ring key on record",
          title: `No PeopleLite.LitePeople or People.AccountToPersonalId record for the queried accounts names a ring key. That is not a finding that the person is outside the rings.\n${RING_NOTE}`,
        },
      ];
}

function chainRows(chain: ChainState): VersionRow[] {
  switch (chain.state) {
    case "idle":
      return [
        {
          label: "People chain",
          value: "Not checked",
          detail: "press Refresh",
          state: "unknown",
        },
      ];
    case "loading":
      return [{ label: "People chain", value: "Checking…", state: "unknown" }];
    case "error":
      return [
        {
          label: "People chain",
          value: "Unavailable",
          detail: chain.message,
          state: "warning",
          title: `${chain.message}\nThe session is unaffected. Nothing was changed.`,
        },
      ];
    case "done":
      return [
        ...chain.reading.readings.map(describeReading),
        slotsRow(chain.reading.readings),
        ...ringRows(chain.reading.readings),
        {
          label: "Checked",
          value: chain.checkedAt.toLocaleTimeString(),
          detail: abbreviate(chain.reading.blockHash),
          title: `${SOURCE}\nBlock ${chain.reading.blockHash}`,
        },
      ];
  }
}

/**
 * Rows for the account status in the menu.
 *
 * The session's own facts come first: the keys come from the recovery phrase
 * and the usernames are whatever the session carries, which is nothing for an
 * imported wallet. Standing comes only from the People chain read, and each
 * queried account is reported by its own role, so a record missing for one is
 * never read as the person being unregistered.
 */
export function accountStatusRows(
  state: AuthState,
  context: AccountContext,
  chain: ChainState = { state: "idle" },
): VersionRow[] {
  const network: VersionRow = {
    label: "Network",
    value: context.networkSuffix,
  };
  if (state.tag !== "Connected") {
    const value =
      state.tag === "Disconnected"
        ? "Signed out"
        : state.tag === "LoginFailed"
          ? `Failed: ${state.value.reason}`
          : state.tag;
    return [network, { label: "Session", value }];
  }
  const { publicKey, identityAccountId, fullUsername, liteUsername } =
    state.value;
  const reported = fullUsername ?? liteUsername;
  return [
    network,
    { label: "Session", value: "Signed in" },
    ...(context.walletName
      ? [{ label: "Wallet", value: context.walletName }]
      : []),
    { label: "Root key", value: abbreviate(publicKey), title: publicKey },
    identityAccountId
      ? {
          label: "Identity account",
          value: abbreviate(identityAccountId),
          title: `${identityAccountId}\nDerived from the recovery phrase for ${context.networkSuffix}.`,
        }
      : {
          label: "Identity account",
          value: "Not reported",
          state: "unknown",
        },
    ...(reported
      ? [
          {
            label: "Session username",
            value: reported,
            detail: fullUsername ? "full" : "lite",
          },
        ]
      : []),
    ...chainRows(chain),
  ];
}

/** The product the status is about, and what the core's ledger records for it. */
export type ProductAllowanceView =
  | { state: "none-open" }
  | { state: "signed-out"; productId: string }
  | { state: "loading"; productId: string }
  | { state: "done"; productId: string; record: AllocationRecord };

/**
 * Rows for the open product's resources.
 *
 * The core's own ledger records whether it completed a Statement Store
 * allocation for this product. It is local and historical: it is written after
 * the chain accepted an allocation and says nothing about the chain now. What
 * the chains hold now comes from `resources`, which the core read for the same
 * product; a read for another product is not shown.
 */
export function productAllowanceRows(
  view: ProductAllowanceView,
  resources: ResourcesState = { state: "idle" },
): VersionRow[] {
  if (view.state === "none-open")
    return [{ label: "Product", value: "None open", state: "unknown" }];
  const product: VersionRow = { label: "Product", value: view.productId };
  if (view.state === "signed-out")
    return [
      product,
      {
        label: "Allocation recorded",
        value: "Sign in first",
        state: "unknown",
      },
    ];
  if (view.state === "loading")
    return [
      product,
      { label: "Allocation recorded", value: "Checking…", state: "unknown" },
    ];
  const record: VersionRow =
    view.record === "recorded"
      ? {
          label: "Allocation recorded",
          value: "Yes",
          title:
            "The core's renewal ledger lists this product. The core writes it only after the allocation reached the chain, for a new slot or one already held. It is a local record, not a check of the chain now, and it does not say the allowance is still current.",
        }
      : view.record === "none"
        ? {
            label: "Allocation recorded",
            value: "No",
            title:
              "This wallet's ledger has no entry for this product. No allocation has completed for it here, which fits a failed allocation but does not say why it failed. It says nothing about the chain.",
          }
        : {
            label: "Allocation recorded",
            value: "Unreadable",
            state: "warning",
            title: "The stored ledger is not the layout this reads.",
          };
  const forThisProduct: ResourcesState =
    resources.state !== "idle" && resources.productId === view.productId
      ? resources
      : { state: "idle" };
  return [product, record, ...resourceRows(forThisProduct)];
}
