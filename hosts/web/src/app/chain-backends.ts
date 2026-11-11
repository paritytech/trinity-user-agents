import {
  CHAIN_ROLES,
  chainBackends,
  type ChainBackend,
  type ChainRole,
  type NetworkConfig,
} from "../platform/network-config.js";
import {
  saveChainBackends,
  savedChainBackends,
} from "../platform/network-choice.js";

/**
 * The menu's Chain connections: for each chain, the backend this tab runs it
 * on now and the one it will run on after a reload. The core and every
 * connection are built at boot, so a choice is saved first and applied by
 * reloading the tab, as a network choice is.
 */

/** How the menu and the Activity log name each chain. */
export const CHAIN_LABELS: Record<ChainRole, string> = {
  relay: "Relay",
  assetHub: "Asset Hub",
  people: "People",
  bulletin: "Bulletin",
};

/** How the menu names each backend. */
export const BACKEND_LABELS: Record<ChainBackend, string> = {
  rpc: "RPC node",
  "light-client": "Light client",
};

/** One chain's line in the menu. */
export interface ChainBackendRow {
  role: ChainRole;
  label: string;
  /** The backend the running connection uses. */
  running: ChainBackend;
  /** The backend saved for this tab, which the next load uses. */
  chosen: ChainBackend;
  /** The network's default for the chain. */
  defaultBackend: ChainBackend;
  /** The chain's RPC node. */
  rpc: string;
  /** Whether the light client can serve this chain on this stack. */
  lightClient: boolean;
}

/** Each chain's running and chosen backend, in the order the host connects them. */
export function chainBackendRows(
  config: NetworkConfig,
  running: Record<ChainRole, ChainBackend>,
  chosen: Record<ChainRole, ChainBackend>,
  lightClient: ReadonlySet<ChainRole> = new Set(CHAIN_ROLES),
): ChainBackendRow[] {
  return CHAIN_ROLES.map((role) => ({
    role,
    label: CHAIN_LABELS[role],
    running: running[role],
    // A chain the light client cannot serve runs on its RPC node whatever was saved.
    chosen: lightClient.has(role) ? chosen[role] : "rpc",
    defaultBackend: config.chains[role].backend,
    rpc: config.chains[role].rpc,
    lightClient: lightClient.has(role),
  }));
}

/** What a row says the chain runs on now, and what a reload changes. */
export function runningText(row: ChainBackendRow): string {
  const now =
    row.running === "rpc" ? `Now: RPC node ${row.rpc}` : "Now: light client";
  return row.chosen === row.running
    ? now
    : `${now} · after reload: ${row.chosen === "rpc" ? "RPC node" : "light client"}`;
}

/** The line under the rows: whether the running connections match the choice. */
export function chainBackendsNote(rows: ChainBackendRow[]): string {
  const waiting = rows.filter((row) => row.chosen !== row.running).length;
  if (waiting === 0)
    return "Running as chosen. Saved for this tab; a change applies when the tab reloads.";
  return `${waiting} change${waiting === 1 ? " waits" : "s wait"} for a reload. Reloading reconnects the core, restarts the session's workers and reopens the product.`;
}

/** The saved choice for `chosen`: only the chains that differ from the network's default. */
export function overridesFor(
  config: NetworkConfig,
  chosen: Record<ChainRole, ChainBackend>,
): Partial<Record<ChainRole, ChainBackend>> {
  return Object.fromEntries(
    CHAIN_ROLES.filter(
      (role) => chosen[role] !== config.chains[role].backend,
    ).map((role) => [role, chosen[role]]),
  );
}

/** The menu's Chain connections section and what it acts on. */
export interface ChainBackendsMenu {
  list: HTMLElement;
  note: HTMLElement;
  apply: HTMLButtonElement;
  defaults: HTMLButtonElement;
  network: NetworkConfig;
  /** The backends this page connected with. */
  running: Record<ChainRole, ChainBackend>;
  /** The chains the light client can serve on this stack. */
  lightClient: ReadonlySet<ChainRole>;
  /** Where the tab's choice is saved. */
  storage: Storage;
  reload: () => void;
}

/**
 * Draw the section and keep it current: a choice is saved for this tab at
 * once, and Reload to apply starts the page again with it.
 */
export function bindChainBackends(menu: ChainBackendsMenu): void {
  const { network, storage } = menu;
  const render = () => {
    const chosen = chainBackends(
      network,
      savedChainBackends(storage, network.id),
    );
    const rows = chainBackendRows(
      network,
      menu.running,
      chosen,
      menu.lightClient,
    );
    const choose = (role: ChainRole, backend: ChainBackend) => {
      saveChainBackends(
        storage,
        network.id,
        overridesFor(network, { ...chosen, [role]: backend }),
      );
      render();
    };
    menu.list.replaceChildren(...rows.map((row) => rowElement(row, choose)));
    menu.note.textContent = chainBackendsNote(rows);
    menu.apply.disabled = rows.every((row) => row.chosen === row.running);
    menu.defaults.disabled = rows.every(
      (row) => row.chosen === row.defaultBackend,
    );
  };
  menu.apply.addEventListener("click", menu.reload);
  menu.defaults.addEventListener("click", () => {
    saveChainBackends(storage, network.id, {});
    render();
  });
  render();
}

function rowElement(
  row: ChainBackendRow,
  choose: (role: ChainRole, backend: ChainBackend) => void,
): HTMLElement {
  const line = document.createElement("div");
  line.className = "chain-backend";
  line.toggleAttribute("data-pending", row.chosen !== row.running);
  const id = `chain-backend-${row.role}`;
  const label = document.createElement("label");
  label.htmlFor = id;
  label.textContent = row.label;
  const select = document.createElement("select");
  select.id = id;
  select.dataset.chain = row.role;
  select.replaceChildren(
    ...(["rpc", "light-client"] as const).map((backend) => {
      const option = new Option(
        `${BACKEND_LABELS[backend]}${backend === row.defaultBackend ? " (default)" : ""}${backend === "light-client" && !row.lightClient ? " (no chain spec)" : ""}`,
        backend,
      );
      option.disabled = backend === "light-client" && !row.lightClient;
      return option;
    }),
  );
  select.value = row.chosen;
  select.addEventListener("change", () =>
    choose(row.role, select.value as ChainBackend),
  );
  const now = document.createElement("p");
  now.className = "note";
  now.textContent = runningText(row);
  line.append(label, select, now);
  return line;
}
