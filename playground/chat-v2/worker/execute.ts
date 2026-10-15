/**
 * The commands of the command table, run against the client. A command that
 * posts no message of its own answers `ok <command>` or
 * `error <command>: <reason>` in the room it came from.
 */
import type {
  ChatMessageStatus,
  ChatRoomCalls,
  ChatRoomConfig,
  ChatRoomFeatures,
  ChatRoomPreview,
  HostChatSetRoomFooterRequest,
} from "@parity/truapi";
import type { RoomId } from "./clock";
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
import type { MenuMode, ReactionsMode } from "./state";

/** The interval between two `/stream` edits: 20 edits a second. */
export const STREAM_INTERVAL_MS = 50;
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

/** `source` keeps its own messages; the other rooms are host-store rooms. */
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
  const modes: readonly MenuMode[] = ["custom", "extend", "hide-edit", "none"];
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

/** The text of the notification `/notify` sends from `room`. */
export function notificationText(room: RoomId): string {
  return `Sample notification from ${room}`;
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
