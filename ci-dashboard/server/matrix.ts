// Posts to Matrix through the host team's relay (paritytech/ci-host-matrix-relay),
// the same service the workflows' notifications use. The relay holds the
// Matrix credential and only posts to rooms on its allowlist; this server only
// holds the relay's API token.

export type Message = { text: string; html: string };

export type Relay = {
  /** The relay's public origin, as in the NOTIFICATION_BOT_URL secret. */
  url: string;
  /** The relay's API token, as in the NOTIFICATION_BOT_TOKEN secret. */
  token: string;
  rooms: string[];
};

type Fetch = typeof fetch;

const MAX_RETRY_WAIT_MS = 30_000;

/** Sends to every room; a room that fails is logged and the rest still get it. */
export async function post(
  relay: Relay,
  message: Message,
  fetchFn: Fetch = fetch,
): Promise<{ room: string; ok: boolean; detail: string }[]> {
  return Promise.all(
    relay.rooms.map(async (room) => {
      const url = `${relay.url.replace(/\/$/, "")}/matrix/send/${encodeURIComponent(room)}`;
      const send = () =>
        fetchFn(url, {
          method: "POST",
          headers: {
            // The relay's contract: the bare token, no Bearer prefix.
            authorization: relay.token,
            "content-type": "application/json",
          },
          body: JSON.stringify({
            text: message.text,
            formatted_text: message.html,
          }),
          signal: AbortSignal.timeout(20_000),
        });
      try {
        let response = await send();
        // The relay passes the homeserver's rate limit back with its wait.
        if (response.status === 429) {
          const seconds = Number(response.headers.get("retry-after"));
          await Bun.sleep(
            Math.min(
              Number.isFinite(seconds) ? seconds * 1000 : 2_000,
              MAX_RETRY_WAIT_MS,
            ),
          );
          response = await send();
        }
        const ok = response.ok;
        if (!ok) {
          const reason = (await response.text().catch(() => "")).slice(0, 200);
          console.error(`Relay ${response.status} for ${room}: ${reason}`);
        }
        return { room, ok, detail: `HTTP ${response.status}` };
      } catch (error) {
        console.error(`Relay send to ${room} failed:`, error);
        return { room, ok: false, detail: String(error) };
      }
    }),
  );
}

export function escape(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

// More than this many in one post reads as a wall; the rest are counted.
const FOLD_LIMIT = 20;

/** Several messages as one post, the oldest first. */
export function fold(messages: Message[]): Message {
  if (messages.length === 1) {
    return messages[0]!;
  }
  const shown = messages.slice(0, FOLD_LIMIT);
  const rest = messages.length - shown.length;
  const more = rest ? `…and ${rest} more` : "";
  return {
    text: [...shown.map((m) => m.text), more].filter(Boolean).join("\n\n"),
    html: [...shown.map((m) => m.html), more].filter(Boolean).join("<br><br>"),
  };
}

/**
 * Posts one message at a time, so each room has at most one post in flight.
 * What arrives within `gatherMs` of the first message, or while a post is
 * being sent, goes out folded into the next post: a burst of new Sentry issues
 * is one post, not a run of them that the homeserver rate limits and drops.
 * The promise settles once the message has been sent.
 */
export function queue(
  send: (message: Message) => Promise<unknown>,
  gatherMs = 2_000,
): (message: Message) => Promise<void> {
  let pending: Message[] = [];
  let draining: Promise<void> | undefined;
  const drain = async () => {
    await Bun.sleep(gatherMs);
    while (pending.length > 0) {
      const batch = pending;
      pending = [];
      try {
        await send(fold(batch));
      } catch (error) {
        console.error("Matrix:", error);
      }
    }
    draining = undefined;
  };
  return (message) => {
    pending.push(message);
    draining ??= drain();
    return draining;
  };
}
