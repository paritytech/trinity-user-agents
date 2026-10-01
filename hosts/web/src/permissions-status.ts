import type {
  PermissionAuthorizationRequest,
  PermissionAuthorizationStatus,
} from "@parity/truapi-host";
import type { VersionRow } from "./versions.js";

/** One permission the core can be asked about, and how it is named here. */
export interface PermissionQuery {
  label: string;
  request: PermissionAuthorizationRequest;
}

const device = (
  label: string,
  permission: Extract<
    PermissionAuthorizationRequest,
    { tag: "Device" }
  >["value"],
): PermissionQuery => ({
  label,
  request: { tag: "Device", value: permission },
});

const remote = (
  label: string,
  tag: "ChainSubmit" | "PreimageSubmit" | "StatementSubmit" | "WebRtc",
): PermissionQuery => ({
  label,
  request: { tag: "Remote", value: { permission: { tag } } },
});

/**
 * The permissions with a fixed name in the protocol, in the order they show.
 * Network access is per domain and has no fixed list, so it is read for the
 * domains this tab saw the core ask about, and not here.
 */
export const CAPABILITIES: PermissionQuery[] = [
  remote("Submit transactions", "ChainSubmit"),
  remote("Submit statements", "StatementSubmit"),
  remote("Submit preimages", "PreimageSubmit"),
  remote("WebRTC", "WebRtc"),
  { label: "Share identity", request: { tag: "IdentityDisclosure" } },
  device("Camera", "Camera"),
  device("Microphone", "Microphone"),
  device("Notifications", "Notifications"),
  device("Location", "Location"),
  device("Clipboard", "Clipboard"),
  device("Open URLs", "OpenUrl"),
  device("Bluetooth", "Bluetooth"),
  device("NFC", "NFC"),
  device("Biometrics", "Biometrics"),
];

/** The query for network access to one domain. */
export function domainQuery(domain: string): PermissionQuery {
  return {
    label: domain,
    request: {
      tag: "Remote",
      value: { permission: { tag: "Remote", value: { domains: [domain] } } },
    },
  };
}

/** The queries for `domains`, without repeats, in the order first seen. */
export function queriesFor(domains: readonly string[]): {
  capabilities: PermissionQuery[];
  networks: PermissionQuery[];
} {
  return {
    capabilities: CAPABILITIES,
    networks: [...new Set(domains)].map(domainQuery),
  };
}

/** The answer the core gave for one query. */
export interface PermissionAnswer {
  query: PermissionQuery;
  status: PermissionAuthorizationStatus;
}

/** Where the permission read stands for the open product. */
export type PermissionsView =
  | { state: "none-open" }
  | { state: "signed-out" }
  | { state: "loading" }
  | { state: "error"; message: string }
  | {
      state: "done";
      capabilities: PermissionAnswer[];
      networks: PermissionAnswer[];
    };

const SCOPE_ROW: VersionRow = {
  label: "Scope",
  value: "Explicit TrUAPI calls",
  title:
    "These answers are for permission requests a product makes through TrUAPI. A page's own fetch, XHR, WebSocket, images and media follow the browser and the site's CORS rules, and camera and microphone also need the browser's and the OS's permission.",
};

const STATUS_TEXT: Record<PermissionAuthorizationStatus, string> = {
  Authorized: "Allowed",
  Denied: "Denied",
  NotDetermined: "Ask",
};

function answerRow(answer: PermissionAnswer, prefix = ""): VersionRow {
  return {
    label: `${prefix}${answer.query.label}`,
    value: STATUS_TEXT[answer.status],
    state: answer.status === "Denied" ? "warning" : undefined,
    title:
      answer.status === "Authorized"
        ? "The core reports this as granted now. That is a saved Always allow or a one-time allow not yet used, and the core does not say which. For devices, the browser and the OS ask separately."
        : answer.status === "Denied"
          ? "A stored denial, or a request the core refuses outright."
          : "No stored or one-time answer. The core asks the next time the product needs it. That is not a denial.",
  };
}

/**
 * Rows for the open product's host permissions.
 *
 * Everything comes from the core's own non-prompting status read, which gives
 * the current answer: a stored decision or a one-time allow not yet used, and
 * it does not say which. "Ask" means neither exists, which is not a denial. Network access is listed only for
 * domains this tab saw the core ask about, because the core has no call that
 * lists them.
 */
export function permissionRows(view: PermissionsView): VersionRow[] {
  switch (view.state) {
    case "none-open":
      return [{ label: "Product", value: "None open", state: "unknown" }];
    case "signed-out":
      return [
        SCOPE_ROW,
        {
          label: "Current permissions",
          value: "Sign in first",
          state: "unknown",
        },
      ];
    case "loading":
      return [
        SCOPE_ROW,
        { label: "Current permissions", value: "Checking…", state: "unknown" },
      ];
    case "error":
      return [
        SCOPE_ROW,
        {
          label: "Current permissions",
          value: "Unavailable",
          detail: view.message,
          state: "warning",
        },
      ];
    case "done": {
      const decided = view.capabilities.filter(
        ({ status }) => status !== "NotDetermined",
      );
      const asking = view.capabilities.filter(
        ({ status }) => status === "NotDetermined",
      );
      return [
        SCOPE_ROW,
        ...decided.map((answer) => answerRow(answer)),
        ...(asking.length > 0
          ? [
              {
                label: "Ask",
                value: asking.map(({ query }) => query.label).join(", "),
                title:
                  "No stored or one-time answer for these. The core asks the next time the product needs one. That is not a denial.",
              },
            ]
          : []),
        ...view.networks.map((answer) => answerRow(answer, "Network: ")),
      ];
    }
  }
}
