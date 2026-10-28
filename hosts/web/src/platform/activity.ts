/**
 * What the host reports to its activity log (`tools/activity/`), one event at
 * a time, as it happens. Producers state the facts they hold, such as the
 * product id, the execution kind or a failure's reason. They do not write
 * sentences for the log to parse.
 *
 * An event holds only what the host may show anyone who opens the developer
 * tools: names, product ids, addresses and reasons. It never holds a key, a
 * recovery phrase, a credential or what a person wrote.
 */

/** Which part of the host an event is about, for the log's filters. */
export type ActivityCategory =
  | "session"
  | "products"
  | "workers"
  | "network"
  | "host";

/** One labelled fact, shown when the event's row is opened. */
export interface ActivityDetail {
  label: string;
  value: string;
}

/** Something the host did or saw. */
export interface Activity {
  category: ActivityCategory;
  /** What happened, such as `Opened` or `Signed in`, or a whole sentence when there is nothing more to it. */
  action: string;
  /** What it happened to: a product, a name or a wallet. */
  subject?: string;
  /** Why it happened, or what came of it, in one line. */
  note?: string;
  /** It failed, or the host refused it. */
  failed?: boolean;
  details?: ActivityDetail[];
}

/** Where producers send their events. */
export type ActivitySink = (activity: Activity) => void;

/** Query parameters whose values are left out of a logged URL. */
const SECRET_PARAM =
  /token|secret|key|pass|auth|credential|signature|sig$|session|code/i;

/**
 * `url` as the log may show it: without a user name or password, and without
 * the values of query parameters whose names suggest a secret. Text that is
 * not an absolute URL is returned as it is.
 */
export function redactUrl(url: string): string {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return url;
  }
  if (parsed.username !== "" || parsed.password !== "") {
    parsed.username = "";
    parsed.password = "";
  }
  for (const name of [...parsed.searchParams.keys()])
    if (SECRET_PARAM.test(name)) parsed.searchParams.set(name, "…");
  return parsed.href;
}

/** Every absolute http(s) or ws(s) URL in `text`, redacted with {@link redactUrl}. */
export const redactUrls = (text: string): string =>
  text.replace(/\b(?:https?|wss?):\/\/[^\s"'<>]+/g, redactUrl);
