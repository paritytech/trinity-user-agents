import { decodeLink } from "@parity/trinity-stack";
import {
  CHAIN_ROLES,
  NETWORKS,
  type NetworkConfig,
} from "../platform/network-config.js";
import { DEFAULT_NETWORK } from "../platform/network-scope.js";
import {
  checkEndpoints,
  type EndpointCheck,
} from "../platform/endpoint-check.js";
import {
  checkCustomStack,
  customRecordId,
  customStacks,
  findCustomStack,
  forgetCustomStack,
  saveCustomStack,
  type CustomStackCheck,
} from "../platform/stacks.js";
import {
  rememberNetwork,
  type NetworkChoice,
} from "../platform/network-choice.js";
import { CHAIN_LABELS } from "./chain-backends.js";

/**
 * The menu's Stack section: the stacks this tab can run on, what the running
 * one is trusted as, and adding a stack by its descriptor URL or stack link.
 * A choice is saved for the tab and applied by reloading it, so the core and
 * every connection are built for one stack.
 */

/** The stacks the menu offers: every built-in, then the custom ones this browser keeps. */
export function offeredStacks(storage: Storage): NetworkChoice[] {
  return [
    ...Object.values(NETWORKS).map((network) => ({
      id: network.id,
      label: network.displayName,
    })),
    ...customStacks(storage).map((record) => {
      const { url } = decodeLink(record.link);
      return {
        id: customRecordId(record),
        label: `${record.pinned.id} (custom, ${new URL(url).host})`,
      };
    }),
  ];
}

const shortKey = (key: string) => `${key.slice(0, 10)}…${key.slice(-6)}`;

/** The built-in stacks that derive accounts under `tld`, other than `id`. */
function sharingTld(tld: string, id: string): string[] {
  return Object.values(NETWORKS)
    .filter((network) => network.networkSuffix === tld && network.id !== id)
    .map((network) => network.displayName);
}

/** Who decides the running stack's chains and services, and what is off on it. */
export function trustText(config: NetworkConfig): string[] {
  const { pointer } = config.stack;
  const lines =
    config.origin === "built-in"
      ? [
          `Built in. Every copy must be signed by the key this build pins (${shortKey(pointer.key!)}).`,
        ]
      : pointer.key !== undefined
        ? [
            `Custom, pinned to key ${shortKey(pointer.key)}, which you accepted. Every chain runs on its RPC node; backup and username search are off.`,
          ]
        : [
            `Custom and unsigned: whoever serves ${pointer.url} decides its chains and services. Every chain runs on its RPC node; backup and username search are off.`,
          ];
  if (config.networkSuffix === "dot")
    lines.push(
      "It uses Polkadot's dot TLD: a recovery phrase derives the same identity here as on Polkadot. Use test phrases only.",
    );
  return lines;
}

/** One row of a stack's summary, before the user adds it. */
export interface SummaryRow {
  label: string;
  value: string;
  /** The row is a reason not to add the stack, or to look twice. */
  tone?: "warning" | "error";
}

/** What the user is shown before adding a stack. */
export function customSummary(
  check: Extract<CustomStackCheck, { kind: "custom" }>,
  endpoints: EndpointCheck[],
): SummaryRow[] {
  const { config } = check;
  const { pointer, descriptor } = config.stack;
  const shared = sharingTld(config.networkSuffix, config.id);
  const byRole = new Map(endpoints.map((entry) => [entry.role, entry]));
  return [
    { label: "Name", value: `${descriptor.name} (${descriptor.id})` },
    { label: "Descriptor", value: pointer.url },
    pointer.key !== undefined
      ? {
          label: "Signed by",
          value: `${pointer.key}. Later copies must be signed by this key. The signature says who published it, not that they are trustworthy.`,
        }
      : {
          label: "Signed by",
          value:
            "Nobody: a plain URL pins no key. Whoever serves it, now or later, decides its chains and services, and can roll it back. Add a stack link with a key to pin its signer.",
          tone: "warning",
        },
    {
      label: "TLD",
      value:
        shared.length === 0
          ? config.networkSuffix
          : `${config.networkSuffix}. A recovery phrase derives the same accounts here as on ${shared.join(", ")}, and a signature not bound to a chain, such as a statement, is valid on both.`,
      tone: shared.length === 0 ? undefined : "warning",
    },
    ...CHAIN_ROLES.map((role): SummaryRow => {
      const endpoint = byRole.get(role);
      return {
        label: CHAIN_LABELS[role],
        value: `${config.chains[role].rpc}${
          endpoint === undefined || endpoint.outcome === "ok"
            ? ""
            : endpoint.outcome === "wrong"
              ? `: wrong chain, ${endpoint.detail}`
              : `: not checked, ${endpoint.detail}`
        }`,
        tone:
          endpoint?.outcome === "wrong"
            ? "error"
            : endpoint?.outcome === "unchecked"
              ? "warning"
              : undefined,
      };
    }),
    {
      label: "Identity backend",
      value: config.identityBackend ?? "none",
    },
    { label: "IPFS gateway", value: config.dotns.contentGateway },
    ...(descriptor.fork === undefined
      ? []
      : [
          {
            label: "Fork of",
            value: `${descriptor.fork.of.url}. Being a fork makes it neither more nor less trusted.`,
          },
        ]),
    ...(check.stale
      ? [
          {
            label: "Stale",
            value: "Past its validUntil: its operator has not republished it.",
            tone: "warning" as const,
          },
        ]
      : []),
    {
      label: "Off on this stack",
      value:
        "The light client (every chain runs on its RPC node), the wallet backup and username search.",
    },
  ];
}

/** The menu's Stack section and what it acts on. */
export interface StackMenu {
  select: HTMLSelectElement;
  trust: HTMLElement;
  form: HTMLFormElement;
  input: HTMLInputElement;
  check: HTMLButtonElement;
  status: HTMLElement;
  summary: HTMLElement;
  actions: HTMLElement;
  use: HTMLButtonElement;
  cancel: HTMLButtonElement;
  forget: HTMLButtonElement;
  /** The stack this tab runs on, or null when the chosen one cannot run. */
  active: NetworkConfig | null;
  /** The id of the stack the tab chose. */
  chosen: string;
  /** Where custom stacks are kept. */
  storage: Storage;
  /** Where the tab's choice is kept. */
  tab: Storage;
  reload: () => void;
}

/** Draw the Stack section and keep it working. */
export function bindStackMenu(menu: StackMenu): void {
  const { storage, tab, active, chosen } = menu;
  const choose = (id: string) => {
    rememberNetwork(tab, id);
    menu.reload();
  };
  menu.select.replaceChildren(
    ...offeredStacks(storage).map(
      (choice) => new Option(choice.label, choice.id),
    ),
  );
  menu.select.value = chosen;
  menu.select.addEventListener("change", () => choose(menu.select.value));
  menu.trust.textContent = active === null ? "" : trustText(active).join(" ");
  menu.forget.hidden = findCustomStack(storage, chosen) === undefined;
  menu.forget.addEventListener("click", () => {
    forgetCustomStack(storage, chosen);
    choose(DEFAULT_NETWORK);
  });

  let pending: (() => void) | null = null;
  const reset = (status = "") => {
    pending = null;
    menu.status.textContent = status;
    menu.status.removeAttribute("data-tone");
    menu.summary.replaceChildren();
    menu.summary.hidden = true;
    menu.actions.hidden = true;
    menu.check.disabled = false;
  };
  const fail = (text: string) => {
    reset(text);
    menu.status.dataset.tone = "error";
  };
  menu.cancel.addEventListener("click", () => reset());
  menu.use.addEventListener("click", () => pending?.());

  menu.form.addEventListener("submit", (event) => {
    event.preventDefault();
    void (async () => {
      reset("Reading the descriptor…");
      menu.check.disabled = true;
      let checked: CustomStackCheck;
      try {
        checked = await checkCustomStack(menu.input.value);
      } catch (error) {
        fail(error instanceof Error ? error.message : String(error));
        return;
      }
      if (checked.kind === "built-in") {
        const builtIn = NETWORKS[checked.id];
        menu.status.textContent = `That is ${builtIn.displayName}, a stack this build ships with its signer pinned.`;
        menu.use.textContent = `Use ${builtIn.displayName}`;
        pending = () => choose(checked.id);
        menu.actions.hidden = checked.id === chosen;
        menu.check.disabled = false;
        return;
      }
      menu.status.textContent = "Asking its nodes which chains they serve…";
      const endpoints = await checkEndpoints(checked.config);
      const rows = customSummary(checked, endpoints);
      menu.summary.replaceChildren(
        ...rows.map((row) => {
          const line = document.createElement("div");
          if (row.tone !== undefined) line.dataset.tone = row.tone;
          const term = document.createElement("dt");
          term.textContent = row.label;
          const value = document.createElement("dd");
          value.textContent = row.value;
          line.append(term, value);
          return line;
        }),
      );
      menu.summary.hidden = false;
      menu.check.disabled = false;
      if (endpoints.some((entry) => entry.outcome === "wrong")) {
        menu.status.textContent =
          "Not added: a node serves another chain than the descriptor says.";
        menu.status.dataset.tone = "error";
        return;
      }
      menu.status.textContent =
        "Check who publishes it and what it changes, then add it. Switching reloads the tab.";
      menu.use.textContent = "Add and use this stack";
      pending = () => {
        saveCustomStack(storage, checked.record);
        choose(checked.config.id);
      };
      menu.actions.hidden = false;
    })();
  });
}
