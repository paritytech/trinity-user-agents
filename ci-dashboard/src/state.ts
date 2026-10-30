import type { CheckResult } from "./checks";

/**
 * Consecutive failures per URL, and whether an alert went out for the
 * current outage. Kept between checks so one blip does not page anyone.
 */
export type State = Record<string, { failures: number; alerted: boolean }>;

/** Failed checks in a row before an alert. */
export const ALERT_AFTER = 2;

export type Transition = {
  kind: "down" | "recovered";
  result: CheckResult;
};

export function advance(
  previous: State,
  results: CheckResult[],
): { state: State; transitions: Transition[] } {
  const state: State = {};
  const transitions: Transition[] = [];
  for (const result of results) {
    const before = previous[result.url] ?? { failures: 0, alerted: false };
    if (result.ok) {
      if (before.alerted) {
        transitions.push({ kind: "recovered", result });
      }
      state[result.url] = { failures: 0, alerted: false };
      continue;
    }
    const failures = before.failures + 1;
    const alert = !before.alerted && failures >= ALERT_AFTER;
    if (alert) {
      transitions.push({ kind: "down", result });
    }
    state[result.url] = { failures, alerted: before.alerted || alert };
  }
  return { state, transitions };
}

export function message(
  transitions: Transition[],
  dashboardUrl: string,
): { text: string; html: string } | undefined {
  if (transitions.length === 0) {
    return undefined;
  }
  const line = ({ kind, result }: Transition) =>
    kind === "down"
      ? `❌ ${result.name} is down: ${result.detail}`
      : `✅ ${result.name} recovered`;
  const htmlLine = ({ kind, result }: Transition) =>
    kind === "down"
      ? `❌ <a href="${escape(result.url)}">${escape(result.name)}</a> is down: ${escape(result.detail)}`
      : `✅ <a href="${escape(result.url)}">${escape(result.name)}</a> recovered`;
  return {
    text: [...transitions.map(line), `Dashboard: ${dashboardUrl}`].join("\n"),
    html: [
      ...transitions.map(htmlLine),
      `<a href="${escape(dashboardUrl)}">Dashboard</a>`,
    ].join("<br>"),
  };
}

function escape(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
