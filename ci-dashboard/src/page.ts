// Browser entry for the dashboard page. The server collects everything:
// GitHub data with its own token, refreshed by webhooks, and the RPC, link,
// certificate and smoke checks on its schedule. The page reads
// it all from /api/dashboard and makes no other calls.

import config from "./config";
import type { Snapshot } from "./data";
import {
  runState,
  type NightlyBuild,
  type RunState,
  type WorkflowRun,
} from "./github";
import type { Status } from "../server/monitor";

const REFRESH_MS = 60_000;
const NIGHTLY_WINDOW_DAYS = 14;
const VIEWS = [
  "home",
  "downloads",
  "nightlies",
  "releases",
  "workflows",
  "nodes",
  "uptime",
  "sentry",
] as const;

type Dashboard = { github: Snapshot | null; checks: Status };

async function load(): Promise<Dashboard> {
  const response = await fetch("api/dashboard", { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  return (await response.json()) as Dashboard;
}

// --- Formatting -------------------------------------------------------------

function escape(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

const relative = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
function ago(iso: string): string {
  // A viewer whose clock runs behind would otherwise see times in the future.
  const minutes = Math.min(
    0,
    Math.round((Date.parse(iso) - Date.now()) / 60_000),
  );
  if (minutes === 0) return "just now";
  if (Math.abs(minutes) < 60) return relative.format(minutes, "minute");
  const hours = Math.round(minutes / 60);
  if (Math.abs(hours) < 48) return relative.format(hours, "hour");
  return relative.format(Math.round(hours / 24), "day");
}

const day = new Intl.DateTimeFormat("en", {
  day: "numeric",
  month: "short",
  year: "numeric",
});

function size(bytes: number): string {
  return `${(bytes / 1_048_576).toFixed(0)} MB`;
}

type PillState = RunState | "up" | "down" | "checking";
const PILL_LABELS: Record<PillState, string> = {
  passing: "Passing",
  failing: "Failing",
  running: "Running",
  unknown: "No result",
  up: "Up",
  down: "Down",
  checking: "Checking",
};
const pill = (state: PillState) =>
  `<span class="pill ${state}">${PILL_LABELS[state]}</span>`;

const buildName = (b: NightlyBuild) =>
  b.build ? `Build ${b.build}` : day.format(new Date(b.publishedAt));

function table(head: string[], rows: string[], numeric: number[] = []) {
  if (rows.length === 0) {
    return `<p class="empty">Nothing to show yet.</p>`;
  }
  const th = head
    .map((h, i) => `<th${numeric.includes(i) ? ' class="num"' : ""}>${h}</th>`)
    .join("");
  return `<table><thead><tr>${th}</tr></thead><tbody>${rows.join("")}</tbody></table>`;
}

// smoke.sh writes Markdown: `name` becomes code.
const markdownCode = (value: string) =>
  escape(value).replace(/`([^`]+)`/g, "<code>$1</code>");

function element(id: string): HTMLElement {
  const found = document.getElementById(id);
  if (!found) throw new Error(`missing #${id}`);
  return found;
}

const checked = (at: string | undefined, every: string) =>
  at ? `Checked by the server ${ago(at)}, ${every}.` : "Not checked yet.";

// "e2e passed" with a link to the run, for the newest Android nightly.
function e2eNote(run: WorkflowRun | undefined): string {
  if (!run) return "";
  const state = runState(run);
  const word = {
    passing: "passed",
    failing: "failed",
    running: "running",
    unknown: "no result",
  }[state];
  return `<a href="${escape(run.html_url)}">e2e ${word}</a>`;
}

// --- Views ------------------------------------------------------------------

function buildRow(b: NightlyBuild): string {
  const downloads = b.assets
    .map(
      (a) =>
        `<a href="${escape(a.url)}">${escape(a.name)}</a> <span class="dim">${size(a.size)}</span>`,
    )
    .join(" ");
  return `<tr>
    <td><span class="badge">${escape(b.label)}</span></td>
    <td><strong>${buildName(b)}</strong></td>
    <td><a href="${escape(b.url)}">${escape(b.tag)}</a></td>
    <td>${ago(b.publishedAt)}</td>
    <td class="downloads">${downloads}</td>
  </tr>`;
}

const BUILD_HEAD = ["Kind", "Build", "Tag", "Published", "Downloads"];

function renderTiles(data: Dashboard | undefined) {
  const github = data?.github ?? undefined;
  const latest = github?.builds.find((b) => b.label === "Android");
  const since = Date.now() - NIGHTLY_WINDOW_DAYS * 86_400_000;
  const recent = github?.builds.filter(
    (b) => b.label === "Android" && Date.parse(b.publishedAt) >= since,
  ).length;
  const failing = github?.runs.filter((r) => runState(r) === "failing").length;
  const rpc = data?.checks.rpc ?? [];
  const up = rpc.filter((n) => n.ok).length;

  const tile = (
    href: string,
    label: string,
    value: string,
    note: string,
    tone = "",
  ) =>
    `<a class="tile" href="${href}"><div class="label">${label}</div><div class="value">${value}</div><div class="note ${tone}">${note}</div></a>`;

  element("tiles").innerHTML = [
    tile(
      "#nightlies",
      "Latest Android nightly",
      latest ? buildName(latest) : "—",
      latest ? ago(latest.publishedAt) : "",
      runState(github?.e2e) === "failing" ? "bad" : "ok",
    ),
    tile(
      "#nightlies",
      `Android nightlies, last ${NIGHTLY_WINDOW_DAYS} days`,
      recent === undefined ? "—" : String(recent),
      "From GitHub releases",
      "accent",
    ),
    tile(
      "#workflows",
      "Workflows failing",
      failing === undefined ? "—" : String(failing),
      `Of ${config.workflows.length} watched`,
      failing ? "bad" : "ok",
    ),
    tile(
      "#nodes",
      "RPC nodes up",
      rpc.length ? `${up} / ${rpc.length}` : "—",
      rpc.length ? "Checked every minute" : "Not checked yet",
      rpc.length ? (up < rpc.length ? "bad" : "ok") : "",
    ),
  ].join("");
}

// A shell line with a button that copies it. Copying can be refused, for
// example outside a secure context, in which case the text is selected so it
// can be copied by hand.
function command(line: string): string {
  return `<div class="command"><code>${escape(line)}</code><button type="button" data-copy="${escape(line)}">Copy</button></div>`;
}

function renderDownloads(github: Snapshot) {
  const { downloads, repository } = config;
  const android = github.builds.find((b) => b.label === "Android");
  const simulator = github.builds.find((b) => b.label === "iOS");
  const { cli } = github;
  const repo = `https://github.com/${repository}`;

  const assetButtons = (build: NightlyBuild | undefined) =>
    build?.assets
      .map(
        (a, i) =>
          `<a class="button${i === 0 ? "" : " secondary"}" href="${escape(a.url)}">Download ${escape(a.name)} <span>${size(a.size)}</span></a>`,
      )
      .join("") ?? "";

  const card = (
    platform: string,
    heading: string,
    meta: string,
    body: string,
  ) => `<article class="download-card">
      <p class="platform">${platform}</p>
      <p class="latest">${heading}</p>
      <p class="dim">${meta}</p>
      ${body}
    </article>`;

  element("downloads-body").innerHTML = [
    card(
      "Android",
      android ? buildName(android) : "No nightly yet",
      android
        ? `Nightly from ${ago(android.publishedAt)} · <a href="${escape(android.url)}">release notes</a>${github.e2e ? ` · ${e2eNote(github.e2e)}` : ""}`
        : "",
      `<div class="actions">${assetButtons(android)}</div>
       <div class="actions"><a class="button secondary" href="obtainium://add/${escape(repo)}">Add to Obtainium</a></div>
       <p class="dim">Obtainium installs each new nightly. After adding, turn on <strong>Include prereleases</strong> and under <strong>APK selection</strong> set <strong>Filter links by regular expression</strong> to <code>${escape(downloads.obtainiumApkFilter)}</code>. The vanilla APK is the build without Google Play services.</p>`,
    ),
    card(
      "iOS",
      simulator ? buildName(simulator) : "No nightly yet",
      simulator
        ? `Simulator nightly from ${ago(simulator.publishedAt)} · <a href="${escape(simulator.url)}">release notes</a>`
        : "",
      `<div class="actions">${assetButtons(simulator)}</div>
       <p class="dim">Install it on a booted simulator:</p>
       ${downloads.simulatorInstall.map(command).join("")}
       <p class="dim">On an iPhone, nightlies come through TestFlight.</p>`,
    ),
    card(
      "CLI",
      cli ? `truapi-host ${escape(cli.version)}` : "truapi-host",
      cli
        ? `Released ${ago(cli.publishedAt)} · <a href="${escape(cli.url)}">release notes</a>`
        : "",
      `<p class="dim">Installs the current stable release:</p>
       ${command(downloads.cliInstall)}`,
    ),
  ].join("");
}

function renderReleases(github: Snapshot) {
  const { releases } = github;
  const latest = new Map<string, (typeof releases)[number]>();
  for (const release of releases) {
    if (!latest.has(release.name)) latest.set(release.name, release);
  }
  element("releases-body").innerHTML = table(
    ["Package", "Latest", "Released"],
    [...latest.values()].map(
      (r) => `<tr>
        <td><code>${escape(r.name)}</code></td>
        <td><a href="${escape(r.url)}"><strong>${escape(r.version)}</strong></a></td>
        <td>${ago(r.publishedAt)}</td>
      </tr>`,
    ),
  );
  element("releases-history").innerHTML = table(
    ["Package", "Version", "Released"],
    releases.map(
      (r) => `<tr>
        <td><code>${escape(r.name)}</code></td>
        <td><a href="${escape(r.url)}">${escape(r.version)}</a></td>
        <td>${ago(r.publishedAt)}</td>
      </tr>`,
    ),
  );
}

function renderGithub(github: Snapshot) {
  renderDownloads(github);
  renderReleases(github);
  element("home-builds").innerHTML = table(
    BUILD_HEAD,
    github.builds.slice(0, 8).map(buildRow),
  );
  element("nightlies-body").innerHTML = table(
    BUILD_HEAD,
    github.builds.map(buildRow),
  );
  element("workflows-body").innerHTML = table(
    ["Workflow", "Latest run", "When", "Trigger", "Branch"],
    config.workflows.map((w, i) => {
      const run = github.runs[i];
      return `<tr>
        <td><a href="https://github.com/${config.repository}/actions/workflows/${w.file}">${escape(w.label)}</a></td>
        <td>${run ? `<a href="${escape(run.html_url)}">${pill(runState(run))}</a>` : pill("unknown")}</td>
        <td>${run ? ago(run.created_at) : ""}</td>
        <td class="dim">${run ? escape(run.event) : ""}</td>
        <td class="dim">${run?.head_branch ? escape(run.head_branch) : ""}</td>
      </tr>`;
    }),
  );
}

function renderSentry() {
  const { sentry } = config;
  const projects = (link: boolean) =>
    table(
      ["Project", "Sentry project"],
      sentry.projects.map(
        (p) => `<tr>
          <td>${escape(p.label)}</td>
          <td>${
            link
              ? `<a href="https://${escape(sentry.organization)}.sentry.io/projects/${escape(p.slug)}/"><code>${escape(p.slug)}</code></a>`
              : `<code>${escape(p.slug)}</code>`
          }</td>
        </tr>`,
      ),
    );
  element("sentry-body").innerHTML = sentry.organization
    ? projects(true)
    : `<p class="empty">Not set up yet. New issues are posted to Matrix once <code>sentry.organization</code> is set in <code>ci-dashboard/config.json</code> and Sentry sends its issue webhook to the server.</p>${projects(false)}`;
}

function renderChecks(checks: Status) {
  const state = (ok: boolean | undefined): PillState =>
    ok === undefined ? "checking" : ok ? "up" : "down";

  element("nodes-body").innerHTML = table(
    ["Endpoint", "Network", "Status", "Block", "Peers"],
    config.rpcEndpoints.map((e) => {
      const n = checks.rpc.find((r) => r.url === e.url);
      return `<tr>
        <td>${escape(e.name)}<div class="dim"><code>${escape(e.url)}</code></div></td>
        <td class="dim">${escape(e.network)}</td>
        <td>${pill(state(n?.ok))}${n?.ok === false ? ` <span class="dim">${escape(n.detail)}</span>` : ""}</td>
        <td class="num">${n?.block?.toLocaleString("en") ?? ""}</td>
        <td class="num">${n?.peers ?? ""}</td>
      </tr>`;
    }),
    [3, 4],
  );
  element("nodes-updated").textContent = checked(
    checks.checkedAt,
    "every minute",
  );

  element("uptime-body").innerHTML = table(
    ["Link", "Status", "Checked by"],
    config.links.map((l) => {
      const result = checks.links.find((r) => r.url === l.url);
      return `<tr>
        <td><a href="${escape(l.url)}">${escape(l.name)}</a></td>
        <td>${pill(state(result?.ok))}${result?.ok === false ? ` <span class="dim">${escape(result.detail)}</span>` : ""}</td>
        <td class="dim">${l.method}${l.lookFor ? `, looks for "${escape(l.lookFor)}"` : ""}</td>
      </tr>`;
    }),
  );
  element("uptime-updated").textContent = checked(
    checks.checkedAt,
    "every minute",
  );
  element("certificates-body").innerHTML = table(
    ["Host", "Status", "Detail"],
    checks.certificates.map(
      (c) => `<tr>
        <td><code>${escape(c.name)}</code></td>
        <td>${pill(state(c.ok))}</td>
        <td class="dim">${escape(c.detail)}</td>
      </tr>`,
    ),
  );
  element("certificates-updated").textContent = checked(
    checks.certificatesCheckedAt,
    "every hour",
  );

  const { smoke } = checks;
  element("smoke-body").innerHTML = smoke
    ? table(
        ["Check", "Result", "Detail"],
        smoke.rows.map(
          (r) => `<tr>
            <td>${markdownCode(r.name)}</td>
            <td>${pill(r.ok ? "passing" : "failing")}</td>
            <td class="dim">${markdownCode(r.detail)}</td>
          </tr>`,
        ),
      )
    : `<p class="empty">The smoke checks have not run yet.</p>`;
  element("smoke-output").hidden = !smoke?.output;
  element("smoke-output-text").textContent = smoke?.output ?? "";
  element("smoke-updated").textContent = smoke
    ? `Ran ${ago(smoke.ranAt)}, each morning after the nightlies. Matrix hears when the result changes.`
    : "";
}

// --- Navigation -------------------------------------------------------------

function show() {
  const wanted = location.hash.slice(1);
  const view = (VIEWS as readonly string[]).includes(wanted) ? wanted : "home";
  for (const name of VIEWS) {
    element(`view-${name}`).hidden = name !== view;
  }
  for (const link of document.querySelectorAll<HTMLAnchorElement>(
    ".sidebar a[data-view]",
  )) {
    if (link.dataset.view === view) {
      link.setAttribute("aria-current", "page");
    } else {
      link.removeAttribute("aria-current");
    }
  }
  const title = element(`view-${view}`).dataset.title;
  document.title = `${title} · Trinity User Agents CI`;
  window.scrollTo(0, 0);
}

// Sidebar groups fold like the admin's; each viewer's choice is remembered.
const GROUPS_KEY = "ci-dashboard:collapsed";

function collapsedGroups(): string[] {
  try {
    return JSON.parse(localStorage.getItem(GROUPS_KEY) ?? "[]") as string[];
  } catch {
    return [];
  }
}

function setUpGroups() {
  const collapsed = new Set(collapsedGroups());
  for (const group of document.querySelectorAll<HTMLElement>(".group")) {
    const name = group.dataset.group ?? "";
    const head = group.querySelector<HTMLButtonElement>(".group-head");
    if (!head) continue;
    head.setAttribute("aria-expanded", String(!collapsed.has(name)));
    head.addEventListener("click", () => {
      const open = head.getAttribute("aria-expanded") !== "true";
      head.setAttribute("aria-expanded", String(open));
      if (open) collapsed.delete(name);
      else collapsed.add(name);
      try {
        localStorage.setItem(GROUPS_KEY, JSON.stringify([...collapsed]));
      } catch {
        // Not remembered; harmless.
      }
    });
  }
}

// --- Start ------------------------------------------------------------------

const GITHUB_VIEWS = [
  "downloads-body",
  "home-builds",
  "nightlies-body",
  "workflows-body",
  "releases-body",
];

async function refresh() {
  let data: Dashboard | undefined;
  try {
    data = await load();
  } catch (error) {
    element("updated").textContent =
      `Could not reach the server: ${String(error)}`;
    return;
  }
  if (data.github) {
    renderGithub(data.github);
  } else {
    for (const id of GITHUB_VIEWS) {
      element(id).innerHTML =
        `<p class="empty">The server is still reading GitHub.</p>`;
    }
  }
  renderChecks(data.checks);
  renderTiles(data);
  element("updated").textContent = data.github
    ? `GitHub data from ${ago(data.github.generatedAt)}`
    : "";
}

element("obtainium-repo").textContent =
  `https://github.com/${config.repository}`;
element("refresh").addEventListener("click", () => void refresh());
window.addEventListener("hashchange", show);
document.addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>(
    "button[data-copy]",
  );
  if (!button) return;
  const line = button.dataset.copy ?? "";
  navigator.clipboard.writeText(line).then(
    () => {
      button.textContent = "Copied";
      setTimeout(() => (button.textContent = "Copy"), 1500);
    },
    () => {
      const code = button.previousElementSibling;
      if (code) getSelection()?.selectAllChildren(code);
    },
  );
});

setUpGroups();
show();
renderTiles(undefined);
renderSentry();
void refresh();
setInterval(() => void refresh(), REFRESH_MS);
