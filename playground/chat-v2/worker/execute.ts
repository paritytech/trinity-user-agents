/**
 * The commands of the command table, run against the client. A command that
 * posts no message of its own answers `ok <command>` or
 * `error <command>: <reason>` in the room it came from.
 */
import type {
  ChatMessageMeta,
  ChatMessageStatus,
  ChatRoomCall,
  ChatRoomCalls,
  ChatRoomConfig,
  ChatRoomFeatures,
  ChatRoomPreview,
  HostChatSetRoomFooterRequest,
} from "@parity/truapi";
import { isThreadRoom, pad, threadRoomId, type RoomId, type ThreadRoomId } from "./clock";
import { commandSpecs, findCommand, helpMarkdown, parseCommand, type ParsedCommand } from "./commands";
import type { ProductContext } from "./context";
import { call, CommandError, describeCallError, reasonOf } from "./errors";
import {
  ACTIVITY_TEXT,
  actionsBody,
  customBody,
  DEEPLINK_TEXT,
  fileBody,
  FOOTER_PLACEHOLDER,
  hostStoreConfig,
  LEGACY_TEXT,
  personaId,
  pngDataUrl,
  REASONING_WORDS,
  THREAD_REPLIERS,
  threadMetadata,
  METADATA_DESCRIPTION,
  PAYMENT_DESTINATION,
  png,
  postTextBody,
  richBody,
  roomMetadata,
  STREAM_WORDS,
  SUGGESTIONS,
  textBody,
  toolbar,
  videoBody,
} from "./fixtures";
import { splitGraphemes } from "./graphemes";
import { roomLink } from "./links";
import { defaultFeatures, type MenuMode, type ReactionsMode, type SearchMode } from "./state";

/** The interval between two `/stream` edits: 20 edits a second. */
export const STREAM_INTERVAL_MS = 50;
/**
 * The interval between two `/reason` edits. The edits share the feed budget
 * of the room with the meta, so they stay under 20 a second.
 */
export const REASON_INTERVAL_MS = 60;
/** How long `/reason` says the product thought. */
export const REASON_DURATION_MS = 4000;
/** The most alternatives `/branches` declares, the bound of the RFC. */
export const MAX_BRANCHES = 64;
/** The upper bounds of numeric arguments. */
export const MAX_HISTORY = 100_000;
export const MAX_GUESTS = 100_000;
export const MAX_TOOLBAR = 16;
export const MAX_SLOW_MS = 60_000;
/** The longest delay of `/notify`: one day. */
export const MAX_NOTIFY_SECONDS = 86_400;

const POST_KINDS = ["text", "rich", "video", "file", "actions", "custom", "legacy"] as const;
type PostKind = (typeof POST_KINDS)[number];

const STATUSES: Record<string, ChatMessageStatus> = {
  sent: "Sent",
  delivered: "Delivered",
  read: "Read",
  failed: "Failed",
};

const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** Run one command in a room and post its reply. */
export async function runCommand(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const definition = findCommand(command.name);
  try {
    if (!definition) throw new CommandError("unknown command");
    if (definition.rooms !== "both" && definition.rooms !== modeOf(room)) {
      throw new CommandError(`only in ${definition.rooms}`);
    }
    await COMMAND_RUNNERS[command.name](ctx, room, command);
    if (!definition.posts) await ctx.feed.reply(room, `ok ${command.line}`);
  } catch (error) {
    await ctx.feed.reply(room, `error ${command.line}: ${reasonOf(error)}`);
  }
}

/** `source` keeps its own messages; the other rooms, threads too, are host-store rooms. */
function modeOf(room: RoomId): "host" | "source" {
  return room === "source" ? "source" : "host";
}

type Runner = (ctx: ProductContext, room: RoomId, command: ParsedCommand) => Promise<void>;

const COMMAND_RUNNERS: Record<string, Runner> = {
  help: (ctx, room) => ctx.feed.post(room, { tag: "RichText", value: { text: helpMarkdown(), media: [] } }).then(() => {}),
  meta: runMeta,
  preview: runPreview,
  post: runPost,
  stream: runStream,
  edit: runEdit,
  delete: async (ctx, room, command) => {
    const [id] = expectArgs(command, 1, "<id>");
    await ctx.feed.remove(room, resolveId(ctx, room, id));
  },
  react: (ctx, room, command) => runReact(ctx, room, command, true),
  unreact: (ctx, room, command) => runReact(ctx, room, command, false),
  status: async (ctx, room, command) => {
    const [id, name] = expectArgs(command, 2, "<id> sent|delivered|read|failed");
    const status = STATUSES[name.toLowerCase()];
    if (!status) throw new CommandError("usage: /status <id> sent|delivered|read|failed");
    await ctx.feed.status(room, resolveId(ctx, room, id), status);
  },
  menu: runMenu,
  toolbar: async (ctx, room, command) => {
    const count = expectCount(command, MAX_TOOLBAR, "<n>");
    await call("set_room_toolbar", ctx.chat.setRoomToolbar({ roomId: room, actions: toolbar(count, ctx.assets, ctx.product) }));
  },
  participants: async (ctx, room, command) => {
    const guests = expectCount(command, MAX_GUESTS, "<n>");
    await setConfig(ctx, room, { participants: true, participantActions: true });
    ctx.settings(room).guests = guests;
  },
  reactions: runReactions,
  footer: runFooter,
  placeholder: runPlaceholder,
  suggest: async (ctx, room, command) => {
    const off = expectSwitch(command, "[off]", true);
    await call("set_room_suggestions", ctx.chat.setRoomSuggestions({ roomId: room, suggestions: off ? [] : [...SUGGESTIONS] }));
  },
  commands: async (ctx, room, command) => {
    const [which] = command.args;
    if (command.args.length > 1 || (which !== undefined && which !== "additions" && which !== "off")) {
      throw usage(command, "[additions|off]");
    }
    const commands = which === "off" ? [] : commandSpecs(which === "additions" ? "additions" : "v2");
    await call("set_room_commands", ctx.chat.setRoomCommands({ roomId: room, commands }));
  },
  activity: async (ctx, room, command) => {
    const off = expectSwitch(command, "on|off", false);
    await setActivity(ctx, room, !off);
    ctx.settings(room).activity = !off;
  },
  allowed: async (ctx, room, command) => {
    const emoji = splitGraphemes(command.rest);
    await setConfig(ctx, room, { allowedReactions: emoji.length > 0 ? emoji : undefined });
  },
  history: async (ctx, room, command) => {
    const count = expectCount(command, MAX_HISTORY, "<n>");
    // The generated history replaces every message of the room, so
    // `/history 0` leaves an empty room. The command posts no reply: the
    // history is its output, and a reply would make the room not empty.
    ctx.store.replaceHistory(count);
    const settings = ctx.settings(room);
    settings.productMessages = [];
    settings.deeplinkMessages = [];
    settings.echoMessages = [];
  },
  slow: async (ctx, _room, command) => {
    ctx.state.slowMs = expectCount(command, MAX_SLOW_MS, "<ms>");
  },
  fail: runFail,
  deeplink: async (ctx, room) => {
    await setConfig(ctx, room, { messageActions: true });
    const id = await ctx.feed.post(room, textBody(DEEPLINK_TEXT));
    ctx.settings(room).deeplinkMessages.push(id);
  },
  echo: async (ctx, room) => {
    const settings = ctx.settings(room);
    settings.echo = !settings.echo;
  },
  caps: runCaps,
  features: runFeatures,
  bot: runBot,
  notify: runNotify,
  search: runSearch,
  thread: async (ctx, room, command) => {
    const [id] = expectArgs(command, 1, "<id>");
    await createThread(ctx, room, resolveId(ctx, room, id));
  },
  branches: runBranches,
  reason: runReason,
  call: runCall,
  wake: async (_ctx, _room, command) => {
    if (command.args.length !== 1 || command.args[0] !== "watch") throw usage(command, "watch");
    // A watch routed to `/-/chat/<room>` needs the receiving API of the
    // Notifications service, which no host serves yet (O28).
    throw new CommandError("this host has no receiving API for a watch");
  },
};

/** The names of the commands this module runs, for the tests. */
export const RUNNABLE_COMMANDS: readonly string[] = Object.keys(COMMAND_RUNNERS);

// ------------------------------------------------------------- arguments

function usage(command: ParsedCommand, hint: string): CommandError {
  return new CommandError(`usage: /${command.name} ${hint}`);
}

function expectArgs(command: ParsedCommand, count: number, hint: string): string[] {
  if (command.args.length < count) throw usage(command, hint);
  return command.args;
}

function expectCount(command: ParsedCommand, max: number, hint: string): number {
  const [value] = command.args;
  if (command.args.length !== 1 || !/^(0|[1-9][0-9]*)$/.test(value) || Number(value) > max) {
    throw usage(command, `${hint}, 0 to ${max}`);
  }
  return Number(value);
}

/**
 * Parse `on`/`off`. With `bareIsOn`, no argument means on and `off` is the
 * only argument. The result is `true` for off.
 */
function expectSwitch(command: ParsedCommand, hint: string, bareIsOn: boolean): boolean {
  const [value] = command.args;
  if (command.args.length === 0 && bareIsOn) return false;
  if (command.args.length === 1 && value === "off") return true;
  if (command.args.length === 1 && value === "on" && !bareIsOn) return false;
  throw usage(command, hint);
}

/** The text after the first `words` words of the command. */
function textAfter(command: ParsedCommand, words: number): string {
  let rest = command.rest;
  for (let i = 0; i < words; i++) rest = rest.replace(/^\S+\s*/, "");
  return rest.trim();
}

/** A message id, or `last` for the newest product message of the room. */
export function resolveId(ctx: ProductContext, room: RoomId, id: string): string {
  if (id !== "last") return id;
  const last = ctx.settings(room).productMessages.at(-1);
  if (!last) throw new CommandError("no product message in this room");
  return last;
}

// ------------------------------------------------------------ declarations

/** Replace the configuration with one change and keep it on success. */
async function setConfig(ctx: ProductContext, room: RoomId, change: Partial<ChatRoomConfig>): Promise<void> {
  const config: ChatRoomConfig = { ...ctx.settings(room).config, ...change };
  if (config.allowedReactions === undefined) delete config.allowedReactions;
  if (config.placeholder === undefined) delete config.placeholder;
  await call("set_room_config", ctx.chat.setRoomConfig({ roomId: room, config }));
  ctx.settings(room).config = config;
}

/** Replace the preview with one change and keep it on success. */
export async function setPreview(ctx: ProductContext, room: RoomId, change: Partial<ChatRoomPreview>): Promise<void> {
  const preview: ChatRoomPreview = { ...ctx.settings(room).preview, ...change };
  if (preview.text === undefined) delete preview.text;
  await call("set_room_preview", ctx.chat.setRoomPreview({ roomId: room, preview }));
  ctx.settings(room).preview = preview;
}

export async function setActivity(ctx: ProductContext, room: RoomId, on: boolean): Promise<void> {
  await call(
    "set_room_activity",
    ctx.chat.setRoomActivity(on ? { roomId: room, activity: { text: ACTIVITY_TEXT } } : { roomId: room }),
  );
}

async function runMeta(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  if (command.rest === "") throw usage(command, "<name>");
  await call(
    "set_room_metadata",
    ctx.chat.setRoomMetadata({
      roomId: room,
      metadata: { name: command.rest, avatar: png(ctx.assets.avatarAlt), description: METADATA_DESCRIPTION },
    }),
  );
}

async function runPreview(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const hint = "pin|unpin|badge <n>|image|none|typing on|off|text <t>";
  const [sub, value] = command.args;
  switch (sub) {
    case "pin":
    case "unpin":
      return setPreview(ctx, room, { order: sub === "pin" ? "PinToTop" : "Timestamp" });
    case "badge": {
      if (command.args.length !== 2 || !/^(0|[1-9][0-9]*)$/.test(value) || Number(value) > 0xffff_ffff) {
        throw usage(command, "badge <n>");
      }
      return setPreview(ctx, room, { badge: { tag: "UnreadCount", value: { count: Number(value) } } });
    }
    case "image":
      return setPreview(ctx, room, { badge: { tag: "Image", value: { resource: png(ctx.assets.badge) } } });
    case "none":
      return setPreview(ctx, room, { badge: { tag: "Hidden" } });
    case "typing":
      if (value !== "on" && value !== "off") throw usage(command, "typing on|off");
      return setPreview(ctx, room, { text: value === "on" ? { text: ACTIVITY_TEXT, ongoing: true } : undefined });
    case "text": {
      const text = textAfter(command, 1);
      if (text === "") throw usage(command, "text <t>");
      return setPreview(ctx, room, { text: { text, ongoing: false } });
    }
    default:
      throw usage(command, hint);
  }
}

async function runMenu(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const modes: readonly MenuMode[] = ["custom", "extend", "hide-edit", "thread", "none"];
  const mode = modes.find((candidate) => candidate === command.args[0]);
  if (!mode || command.args.length !== 1) throw usage(command, modes.join("|"));
  await setConfig(ctx, room, { messageActions: mode !== "none" });
  ctx.settings(room).menu = mode;
}

async function runReactions(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const modes: readonly ReactionsMode[] = ["on", "na", "off"];
  const mode = modes.find((candidate) => candidate === command.args[0]);
  if (!mode || command.args.length !== 1) throw usage(command, modes.join("|"));
  await setConfig(ctx, room, { reactionDetails: mode !== "off" });
  ctx.settings(room).reactions = mode;
}

async function runFooter(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const footers: Record<string, HostChatSetRoomFooterRequest["footer"]> = {
    input: { tag: "Input", value: { placeholder: FOOTER_PLACEHOLDER, photo: false, video: false, file: false } },
    photo: { tag: "Input", value: { photo: true, video: true, file: true } },
    empty: { tag: "Empty" },
    custom: { tag: "Custom" },
    pay: {
      tag: "Input",
      value: { photo: true, video: true, file: false, payment: { tag: "Enabled", value: { destination: PAYMENT_DESTINATION } } },
    },
  };
  const footer = command.args.length === 1 ? footers[command.args[0]] : undefined;
  if (!footer) throw usage(command, Object.keys(footers).join("|"));
  await call("set_room_footer", ctx.chat.setRoomFooter({ roomId: room, footer }));
}

/**
 * Set the placeholder of `host` and `source` from either room, and of
 * `host-empty`, which the first `/placeholder` creates.
 */
async function runPlaceholder(ctx: ProductContext, _room: RoomId, command: ParsedCommand): Promise<void> {
  const placeholder = command.rest === "" ? undefined : command.rest;
  await setConfig(ctx, "host", { placeholder });
  await setConfig(ctx, "source", { placeholder });
  if (!ctx.state.hostEmptyCreated) {
    const config: ChatRoomConfig = { ...hostStoreConfig(), ...(placeholder ? { placeholder } : {}) };
    const created = await call(
      "create_room",
      ctx.chat.createRoom({ roomId: "host-empty", metadata: roomMetadata("host-empty", ctx.assets), config }),
    );
    ctx.state.hostEmptyCreated = true;
    if (created.status === "New") {
      ctx.settings("host-empty").config = config;
      return;
    }
  }
  await setConfig(ctx, "host-empty", { placeholder });
}

async function runFail(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const kind = command.args[0];
  if (command.args.length !== 1 || !(kind in ctx.state.fail)) {
    throw usage(command, "reject|timeout|crash|unavailable");
  }
  if (kind !== "unavailable" && room !== "source") throw new CommandError("only in source");
  ctx.state.fail[kind as keyof typeof ctx.state.fail] = true;
}

// ------------------------------------------------------- chat additions

/**
 * `true` for the refusal of a host that does not know the call: a host
 * before the additions answers `Unsupported`, and a core with the default
 * body answers `HostFailure` with the reason `unavailable`.
 */
export function isUnavailable(error: unknown): boolean {
  const { tag, value } = (error ?? {}) as { tag?: string; value?: { reason?: string } };
  return tag === "Unsupported" || (tag === "HostFailure" && value?.reason === "unavailable");
}

/** The one line `/caps` posts for the capabilities of the host. */
export function capsText(features: readonly string[], inline: number, preimage: number): string {
  const names = features.length === 0 ? "none" : features.join(", ");
  return `caps: ${names}; inline ${inline} B; preimage ${preimage} B`;
}

/** Post the Chat features the host serves, or `caps unavailable`. */
async function runCaps(ctx: ProductContext, room: RoomId): Promise<void> {
  const result = await ctx.chat.capabilities();
  if (result.isErr()) {
    if (!isUnavailable(result.error)) throw new CommandError(`capabilities ${describeCallError(result.error)}`);
    await ctx.feed.post(room, textBody("caps unavailable"));
    return;
  }
  const { features, inlineAttachmentMaxBytes, preimageAttachmentMaxBytes } = result.value;
  await ctx.feed.post(room, {
    tag: "RichText",
    value: { text: capsText(features, inlineAttachmentMaxBytes, preimageAttachmentMaxBytes), media: [] },
  });
}

const CALLS: Record<string, ChatRoomCalls | undefined> = {
  audio: { audio: true, video: false },
  video: { audio: false, video: true },
  both: { audio: true, video: true },
  off: undefined,
};

/**
 * Declare one optional feature of the room and keep the others as last
 * declared. The host keeps a feature it does not serve.
 */
async function runFeatures(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const hint = "preimage|search|readaloud on|off, calls audio|video|both|off";
  const [name, value] = command.args;
  if (command.args.length !== 2) throw usage(command, hint);
  const features: ChatRoomFeatures = { ...ctx.settings(room).features };
  const flags: Record<string, "preimageAttachments" | "search" | "readAloud"> = {
    preimage: "preimageAttachments",
    search: "search",
    readaloud: "readAloud",
  };
  if (name === "calls") {
    if (!(value in CALLS)) throw usage(command, hint);
    const calls = CALLS[value];
    if (calls) features.calls = calls;
    else delete features.calls;
  } else if (name in flags && (value === "on" || value === "off")) {
    features[flags[name]] = value === "on";
  } else {
    throw usage(command, hint);
  }
  await call("set_room_features", ctx.chat.setRoomFeatures({ roomId: room, features }));
  ctx.settings(room).features = features;
}

/**
 * `/bot add <id> <name>` registers a persona and posts `bot <id> New` or
 * `bot <id> Exists`; `/bot post <id> <text>` posts as `bot:<id>`;
 * `/bot typing <id>` shows the activity line of `bot:<id>`.
 */
async function runBot(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const hint = "add <id> <name>|post <id> <text>|typing <id>";
  const [sub, botId] = command.args;
  if (botId === undefined) throw usage(command, hint);
  switch (sub) {
    case "add": {
      const name = textAfter(command, 2);
      if (name === "") throw usage(command, "add <id> <name>");
      const registered = await call(
        "register_bot",
        ctx.chat.registerBot({ botId, name, icon: pngDataUrl(ctx.assets.avatarAlt) }),
      );
      if (!ctx.state.bots.includes(botId)) ctx.state.bots.push(botId);
      await ctx.feed.post(room, textBody(`bot ${botId} ${registered.status}`));
      return;
    }
    case "post": {
      const text = textAfter(command, 2);
      if (text === "") throw usage(command, "post <id> <text>");
      await ctx.feed.post(room, textBody(text), { origin: personaId(botId) });
      return;
    }
    case "typing":
      if (command.args.length !== 2) throw usage(command, "typing <id>");
      await call(
        "set_room_activity",
        ctx.chat.setRoomActivity({ roomId: room, activity: { text: ACTIVITY_TEXT, participantId: personaId(botId) } }),
      );
      await ctx.feed.reply(room, `ok ${command.line}`);
      return;
    default:
      throw usage(command, hint);
  }
}

/**
 * `/search on|na|off`: declare `search` in the features of the room, and
 * answer `search_query` from the stored messages (`on`) or with
 * `NotAvailable` (`na`, which keeps `search` declared).
 */
async function runSearch(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const modes: readonly SearchMode[] = ["on", "na", "off"];
  const mode = modes.find((candidate) => candidate === command.args[0]);
  if (!mode || command.args.length !== 1) throw usage(command, modes.join("|"));
  const features: ChatRoomFeatures = { ...ctx.settings(room).features, search: mode !== "off" };
  await call("set_room_features", ctx.chat.setRoomFeatures({ roomId: room, features }));
  ctx.settings(room).features = features;
  ctx.settings(room).search = mode;
}

/** The text of the notification `/notify` sends from `room`. */
export function notificationText(room: RoomId): string {
  return `Sample notification from ${room}`;
}

/** The text of the notification the product sends when a watch wakes it. */
export function wokenNotificationText(room: RoomId, watchId: string): string {
  return `Sample woken in ${room} by ${watchId}`;
}

/**
 * The answer to `Woken`: the preview "Woken by <watch>", the message "woken"
 * in a room the host keeps, and a notification into the room, which the host
 * does not show while the room is muted.
 */
export async function answerWoken(ctx: ProductContext, room: RoomId, watchId: string): Promise<void> {
  await setPreview(ctx, room, { text: { text: `Woken by ${watchId}`, ongoing: false } });
  if (modeOf(room) === "host") await ctx.feed.post(room, textBody("woken"));
  await call(
    "send_push_notification",
    ctx.notifications.sendPushNotification({ text: wokenNotificationText(room, watchId), deeplink: roomLink(room, ctx.product) }),
  );
}

/**
 * `/notify [<seconds>]`: a notification whose deeplink opens the room, now
 * or after `<seconds>`. The host shows nothing for an immediate one into a
 * room the user muted. The delay is the one value the product takes from
 * the clock.
 */
async function runNotify(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const seconds = command.args.length === 0 ? 0 : expectCount(command, MAX_NOTIFY_SECONDS, "[<seconds>]");
  await call(
    "send_push_notification",
    ctx.notifications.sendPushNotification({
      text: notificationText(room),
      deeplink: roomLink(room, ctx.product),
      ...(seconds > 0 ? { scheduledAt: BigInt(Date.now() + seconds * 1000) } : {}),
    }),
  );
}

// ----------------------------------------------------- threads and meta

/**
 * Replace the meta of one message with one change, the other fields as set
 * last, and keep it.
 */
export async function setMeta(
  ctx: ProductContext,
  room: RoomId,
  messageId: string,
  change: Partial<ChatMessageMeta>,
): Promise<void> {
  const meta: ChatMessageMeta = { ...ctx.settings(room).meta[messageId], ...change };
  for (const key of ["thread", "branch", "reasoning"] as const) if (meta[key] === undefined) delete meta[key];
  await call("set_message_meta", ctx.chat.setMessageMeta({ roomId: room, messageId, meta }));
  ctx.settings(room).meta[messageId] = meta;
}

/**
 * The thread of message `messageId` of `room`: a host-store room
 * `thread-<room>-<id>` whose features name the message, three replies from
 * `p1`, `p2` and `bot:helper` (the product while no `helper` persona is
 * registered), and the summary of the root.
 */
export async function createThread(ctx: ProductContext, room: RoomId, messageId: string): Promise<ThreadRoomId> {
  if (isThreadRoom(room)) throw new CommandError("a thread has no thread of its own");
  const thread = threadRoomId(room, messageId);
  const created = await call(
    "create_room",
    ctx.chat.createRoom({ roomId: thread, metadata: threadMetadata(room, messageId, ctx.assets), config: hostStoreConfig() }),
  );
  if (created.status === "Exists") throw new CommandError(`${thread} exists`);
  const features = { ...defaultFeatures(), threadOf: { roomId: room, messageId } };
  await call("set_room_features", ctx.chat.setRoomFeatures({ roomId: thread, features }));
  ctx.settings(thread).features = features;
  const persona = ctx.state.bots.includes("helper") ? personaId("helper") : "product";
  const repliers = [...THREAD_REPLIERS, persona];
  for (const [index, origin] of repliers.entries()) {
    await ctx.feed.post(thread, textBody(`Reply ${index + 1} from ${origin}`), { origin });
  }
  await setMeta(ctx, room, messageId, {
    thread: {
      roomId: thread,
      replyCount: repliers.length,
      lastReplyAt: ctx.clock.now(),
      participantIds: repliers.filter((origin) => origin !== "product"),
    },
  });
  return thread;
}

/** `/branches <id> <n>`: message `<id>` shows alternative 1 of `n`. */
async function runBranches(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const [id, count] = expectArgs(command, 2, "<id> <n>");
  if (!/^[1-9][0-9]*$/.test(count) || Number(count) > MAX_BRANCHES) throw usage(command, `<id> <n>, 1 to ${MAX_BRANCHES}`);
  await setMeta(ctx, room, resolveId(ctx, room, id), { branch: { index: 1, count: Number(count) } });
}

/** The answer to `BranchSelected`: the alternative's text, then its position. */
export async function selectBranch(ctx: ProductContext, room: RoomId, messageId: string, index: number): Promise<void> {
  const branch = ctx.settings(room).meta[messageId]?.branch;
  if (!branch) throw new CommandError(`${messageId} has no branches`);
  if (index < 1 || index > branch.count) throw new CommandError(`${messageId} has no branch ${index}`);
  await ctx.feed.edit(room, messageId, textBody(`Branch ${index} of ${branch.count}`));
  await setMeta(ctx, room, messageId, { branch: { index, count: branch.count } });
}

/**
 * `/reason`: a reasoning row that streams twenty edits while `Preparing`,
 * then is `Sent` with the time the product thought.
 */
async function runReason(ctx: ProductContext, room: RoomId): Promise<void> {
  const id = await ctx.feed.post(room, textBody(REASONING_WORDS[0]), { status: "Preparing" });
  await setMeta(ctx, room, id, { reasoning: {} });
  for (let words = 2; words <= REASONING_WORDS.length; words++) {
    await delay(REASON_INTERVAL_MS);
    await ctx.feed.edit(room, id, textBody(REASONING_WORDS.slice(0, words).join(" ")));
  }
  await ctx.feed.status(room, id, "Sent");
  await setMeta(ctx, room, id, { reasoning: { durationMs: REASON_DURATION_MS } });
}

// ------------------------------------------------------------------ calls

/** Set the call of the room, or clear it with `undefined`, and keep it. */
export async function setCall(ctx: ProductContext, room: RoomId, roomCall: ChatRoomCall | undefined): Promise<void> {
  await call("set_room_call", ctx.chat.setRoomCall(roomCall ? { roomId: room, call: roomCall } : { roomId: room }));
  ctx.settings(room).call = roomCall;
}

/**
 * A call without a media session, which the host shows as a call bar. The
 * start is the clock's time, so the host's timer counts from now.
 */
export function activeCall(callId: string, video: boolean): ChatRoomCall {
  return { tag: "Active", value: { callId, video, startedAt: BigInt(Date.now()) } };
}

/** The id of the next call the product starts: `call-<n>`. */
export function nextCallId(ctx: ProductContext): string {
  ctx.state.calls += 1;
  return `call-${pad(ctx.state.calls, 4)}`;
}

/**
 * `/call ring [video]` rings from `p1`, `/call active [video]` shows a call
 * bar, `/call end` clears the call.
 */
async function runCall(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const hint = "ring [video]|active [video]|end";
  const [state, kind] = command.args;
  const video = kind === "video";
  if (command.args.length > 2 || (kind !== undefined && (kind !== "video" || state === "end"))) throw usage(command, hint);
  switch (state) {
    case "ring":
      return setCall(ctx, room, { tag: "Ringing", value: { callId: nextCallId(ctx), video, caller: "p1" } });
    case "active":
      return setCall(ctx, room, activeCall(nextCallId(ctx), video));
    case "end":
      return setCall(ctx, room, undefined);
    default:
      throw usage(command, hint);
  }
}

// ---------------------------------------------------------------- messages

async function runPost(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const [kind] = command.args;
  if (command.args.length !== 1) throw usage(command, [...POST_KINDS, "all"].join("|"));
  if (kind === "all") {
    // Every kind in order. A kind that fails answers its own error and the
    // rest still run.
    for (const each of POST_KINDS) {
      try {
        await postKind(ctx, room, each);
      } catch (error) {
        await ctx.feed.reply(room, `error /post ${each}: ${reasonOf(error)}`);
      }
    }
    return;
  }
  const known = POST_KINDS.find((candidate) => candidate === kind);
  if (!known) throw usage(command, [...POST_KINDS, "all"].join("|"));
  await postKind(ctx, room, known);
}

async function postKind(ctx: ProductContext, room: RoomId, kind: PostKind): Promise<void> {
  switch (kind) {
    case "text":
      await ctx.feed.post(room, postTextBody);
      return;
    case "rich":
      await ctx.feed.post(room, richBody(ctx.assets, await ctx.preimageKey()));
      return;
    case "video":
      await ctx.feed.post(room, videoBody(ctx.assets));
      return;
    case "file":
      await ctx.feed.post(room, fileBody(ctx.assets));
      return;
    case "actions":
      await ctx.feed.post(room, actionsBody());
      return;
    case "custom":
      await ctx.feed.post(room, customBody);
      return;
    case "legacy": {
      // `post_message` in a product-source room is refused by the host.
      const posted = await call(
        "post_message",
        ctx.chat.postMessage({ roomId: room, payload: { tag: "Text", value: { text: LEGACY_TEXT } } }),
      );
      ctx.settings(room).productMessages.push(posted.messageId);
      return;
    }
  }
}

async function runStream(ctx: ProductContext, room: RoomId): Promise<void> {
  const id = await ctx.feed.post(room, textBody("…"), { status: "Preparing" });
  for (let words = 1; words <= STREAM_WORDS.length; words++) {
    await delay(STREAM_INTERVAL_MS);
    await ctx.feed.edit(room, id, textBody(STREAM_WORDS.slice(0, words).join(" ")));
  }
  await ctx.feed.status(room, id, "Sent");
}

async function runEdit(ctx: ProductContext, room: RoomId, command: ParsedCommand): Promise<void> {
  const [id] = expectArgs(command, 2, "<id> <text>");
  await ctx.feed.edit(room, resolveId(ctx, room, id), textBody(textAfter(command, 1)));
}

async function runReact(ctx: ProductContext, room: RoomId, command: ParsedCommand, placed: boolean): Promise<void> {
  const [id, emoji] = expectArgs(command, 2, "<id> <emoji>");
  await ctx.feed.react(room, resolveId(ctx, room, id), "p1", emoji, placed);
}

/** Run `/help`, as the `sg-event` quick reply asks. */
export function helpCommand(): ParsedCommand {
  const help = parseCommand("/help");
  if (!help) throw new Error("the /help command does not parse");
  return help;
}
