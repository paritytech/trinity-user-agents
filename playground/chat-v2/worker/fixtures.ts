/**
 * The fixed values of the product: participants, menus, toolbar, quick
 * replies, room declarations, message bodies and the generated history.
 */
import type {
  ChatActionHandler,
  ChatMessageBody,
  ChatParticipantEntry,
  ChatProductAction,
  ChatResource,
  ChatRoomConfig,
  ChatRoomMetadata,
  ChatRoomPreview,
  ChatSuggestion,
  ChatToolbarAction,
} from "@parity/truapi";
import type { HexString } from "@parity/truapi/scale";
import { bytesToBase64, hexLength, hexToBytes, repeatedByte, utf8ToHex } from "./bytes";
import { historyId, historyTimestamp, pad, type RoomId } from "./clock";
import { appDeeplink } from "./links";
import type { StoredMessage } from "./store";

/** The asset bytes, as hex. `index.ts` reads them from `assets/`. */
export interface Assets {
  avatar: HexString;
  avatarAlt: HexString;
  badge: HexString;
  /** `icon-1.png` to `icon-6.png`. */
  icons: readonly HexString[];
  samplePng: HexString;
  samplePdf: HexString;
  sampleMp4: HexString;
}

/** The bound of the RFC for an inline avatar, icon or badge image. */
export const INLINE_IMAGE_LIMIT = 64 * 1024;
/** The bound of the RFC for an inline media item. */
export const INLINE_MEDIA_LIMIT = 1024 * 1024;

export function inline(mimeType: string, bytes: HexString): ChatResource {
  return { tag: "Inline", value: { mimeType, bytes } };
}

export function png(bytes: HexString): ChatResource {
  return inline("image/png", bytes);
}

/** A PNG as a `data:` URL, the form `register_bot` takes for an icon. */
export function pngDataUrl(bytes: HexString): string {
  return `data:image/png;base64,${bytesToBase64(hexToBytes(bytes))}`;
}

/** The participant id a product speaks as for its persona `botId`. */
export function personaId(botId: string): string {
  return `bot:${botId}`;
}

/** The preimage key the product uses when the host refuses the submit. */
export const MISSING_PREIMAGE_KEY = repeatedByte(0, 32);

/** BlurHash strings for the media placeholders. */
export const BLURHASH_IMAGE = "LEHV6nWB2yk8pyo0adR*.7kCMdnj";
export const BLURHASH_VIDEO = "L6Pj0^jE.AyE_3t7t7R**0o#DgR4";

// ---------------------------------------------------------------- rooms

export const ROOM_NAMES: Record<"host" | "source" | "host-empty", string> = {
  host: "Sample: host store",
  source: "Sample: product source",
  "host-empty": "Sample: empty",
};

export function roomMetadata(room: "host" | "source" | "host-empty", assets: Assets): ChatRoomMetadata {
  const descriptions: Record<"host" | "source" | "host-empty", string> = {
    host: "The host stores the messages of this room.",
    source: "The sample product keeps the messages of this room.",
    "host-empty": "A host-store room with no messages, for the placeholder.",
  };
  return { name: ROOM_NAMES[room], avatar: png(assets.avatar), description: descriptions[room] };
}

/** The metadata of the thread of message `messageId` of `room`. */
export function threadMetadata(room: RoomId, messageId: string, assets: Assets): ChatRoomMetadata {
  return { name: "Thread", avatar: png(assets.avatar), description: `Replies to ${messageId} in ${room}` };
}

/** The participants who answer in a thread `/thread` creates, in order. */
export const THREAD_REPLIERS: readonly string[] = ["p1", "p2"];

/** The twenty-one words a reasoning row `/reason` streams. */
export const REASONING_WORDS: readonly string[] = (
  "The user asked for a sample so I first check the room then pick a message " +
  "and finally write an answer"
).split(" ");

/** The configuration of a host-store room: every field at its default. */
export function hostStoreConfig(): ChatRoomConfig {
  return {
    productSource: false,
    reactionDetails: false,
    participants: false,
    messageActions: false,
    participantActions: false,
    reportSeen: false,
  };
}

/** The configuration of `source`: the product serves every part. */
export function productSourceConfig(): ChatRoomConfig {
  return {
    productSource: true,
    reactionDetails: true,
    participants: true,
    messageActions: true,
    participantActions: true,
    reportSeen: true,
  };
}

/** The preview `source` declares at creation. */
export function sourcePreview(): ChatRoomPreview {
  return { order: "Timestamp", badge: { tag: "UnreadCount", value: { count: 0 } } };
}

/** The preview state of a room that declares none: the host default. */
export function defaultPreview(): ChatRoomPreview {
  return { order: "Timestamp" };
}

export const METADATA_DESCRIPTION = "Renamed by /meta";

// ---------------------------------------------------------- participants

/** The participants every room has, before `/participants` adds guests. */
export const FIXED_PARTICIPANTS: readonly ChatParticipantEntry[] = [
  { participantId: "p1", participant: { tag: "Person", value: { personId: 1 } } },
  { participantId: "p2", participant: { tag: "LitePerson", value: { account: repeatedByte(2, 32) } } },
  { participantId: "p3", participant: { tag: "Contact", value: { handle: { bytes: repeatedByte(3, 32) } } } },
  { participantId: "p4", participant: { tag: "Product", value: { productId: "truapi-playground.dot" } } },
];

/** How many participants a room has with `extra` guests. */
export function participantCount(extra: number): number {
  return FIXED_PARTICIPANTS.length + extra;
}

/** Participant `index`, from 1: `p1` to `p4`, then the guests `p5` onward. */
export function participantAt(index: number): ChatParticipantEntry {
  if (index <= FIXED_PARTICIPANTS.length) return FIXED_PARTICIPANTS[index - 1];
  return {
    participantId: `p${index}`,
    participant: { tag: "Custom", value: { nickname: `Guest ${index}` } },
  };
}

/** The index of a participant id, or `undefined` for an id of no participant. */
export function participantIndex(participantId: string, extra: number): number | undefined {
  const match = /^p([1-9][0-9]*)$/.exec(participantId);
  if (!match) return undefined;
  const index = Number(match[1]);
  return index <= participantCount(extra) ? index : undefined;
}

// ------------------------------------------------------------------ menus

const CALLBACK: ChatActionHandler = { tag: "Callback" };

export const ACTION_PIN: ChatProductAction = { actionId: "act-pin", label: "Pin", destructive: false, handler: CALLBACK };
export const ACTION_REPORT: ChatProductAction = { actionId: "act-report", label: "Report", destructive: true, handler: CALLBACK };
export const ACTION_MENTION: ChatProductAction = { actionId: "act-mention", label: "Mention", destructive: false, handler: CALLBACK };
export const ACTION_THREAD: ChatProductAction = { actionId: "act-thread", label: "Reply in thread", destructive: false, handler: CALLBACK };

export function actionOpenApp(product?: string): ChatProductAction {
  return {
    actionId: "act-open-app",
    label: "Open the App",
    destructive: false,
    handler: { tag: "Deeplink", value: { url: appDeeplink(product) } },
  };
}

/** The toolbar of `/toolbar n`: `tb-1` opens the App, the rest are callbacks. */
export function toolbar(count: number, assets: Assets, product?: string): ChatToolbarAction[] {
  return Array.from({ length: count }, (_, i) => ({
    actionId: `tb-${i + 1}`,
    label: `Button ${i + 1}`,
    icon: png(assets.icons[i % assets.icons.length]),
    handler: i === 0 ? { tag: "Deeplink", value: { url: appDeeplink(product) } } : CALLBACK,
  }));
}

export const SUGGESTIONS: readonly ChatSuggestion[] = [
  { suggestionId: "sg-send", text: "Say hello", sendsText: true },
  { suggestionId: "sg-event", text: "Show help", sendsText: false },
];

export const ACTIVITY_TEXT = "Sample is typing";
export const FOOTER_PLACEHOLDER = "Message the sample";
export const PAYMENT_DESTINATION = repeatedByte(5, 32);

// ----------------------------------------------------------------- bodies

/** The 40 words `/stream` sends one by one. */
export const STREAM_WORDS: readonly string[] = (
  "The sample product streams this answer one word at a time so that the host " +
  "can show how one bubble grows in place while the product edits it twenty " +
  "times a second and marks it as sent at the end"
).split(" ");

export function textBody(text: string): ChatMessageBody {
  return { tag: "Text", value: { text } };
}

export function postTextBody(counter: number): ChatMessageBody {
  return textBody(`Plain text message ${pad(counter, 4)}. A link: https://parity.io`);
}

export const RICH_MARKDOWN = [
  "# Rich text sample",
  "",
  "A paragraph with **bold**, *italic*, `inline code` and a link to https://parity.io.",
  "",
  "- first item",
  "- second item with **bold**",
  "",
  "1. one",
  "2. two",
  "",
  "> A block quote.",
  "",
  "```ts",
  'const sample = "fenced code";',
  "```",
  "",
  "| Column | Value |",
  "| ------ | ----- |",
  "| table  | text  |",
].join("\n");

export function richBody(assets: Assets, preimageKey: HexString): ChatMessageBody {
  return {
    tag: "RichText",
    value: {
      text: RICH_MARKDOWN,
      media: [
        {
          tag: "Image",
          value: {
            resource: png(assets.samplePng),
            meta: imageMeta(BigInt(hexLength(assets.samplePng)), "Inline sample image"),
          },
        },
        {
          tag: "Image",
          value: {
            resource: { tag: "Preimage", value: { key: preimageKey } },
            meta: imageMeta(BigInt(hexLength(assets.samplePng)), "Preimage sample image"),
          },
        },
      ],
    },
  };
}

function imageMeta(sizeBytes: bigint, alt: string) {
  return { mimeType: "image/png", sizeBytes, width: 320, height: 200, placeholder: BLURHASH_IMAGE, alt };
}

export function videoBody(assets: Assets): ChatMessageBody {
  return {
    tag: "RichText",
    value: {
      text: "A sample video",
      media: [
        {
          tag: "Video",
          value: {
            resource: inline("video/mp4", assets.sampleMp4),
            meta: {
              mimeType: "video/mp4",
              sizeBytes: BigInt(hexLength(assets.sampleMp4)),
              width: 160,
              height: 90,
              placeholder: BLURHASH_VIDEO,
              alt: "Sample video",
            },
          },
        },
      ],
    },
  };
}

export function fileBody(assets: Assets): ChatMessageBody {
  return {
    tag: "RichText",
    value: {
      text: "A sample file",
      media: [
        {
          tag: "File",
          value: {
            resource: inline("application/pdf", assets.samplePdf),
            meta: { mimeType: "application/pdf", sizeBytes: BigInt(hexLength(assets.samplePdf)), alt: "Sample PDF" },
            fileName: "sample.pdf",
          },
        },
      ],
    },
  };
}

export const ACTION_YES = { actionId: "act-yes", title: "Yes" };
export const ACTION_NO = { actionId: "act-no", title: "No" };

export function actionsBody(): ChatMessageBody {
  return { tag: "Actions", value: { text: "Pick one", actions: [ACTION_YES, ACTION_NO], layout: "Column" } };
}

export function pickedBody(actionId: string): ChatMessageBody | undefined {
  const action = [ACTION_YES, ACTION_NO].find((candidate) => candidate.actionId === actionId);
  if (!action) return undefined;
  return { tag: "Actions", value: { text: `Picked ${action.title}`, actions: [], layout: "Column" } };
}

export const CARD_MESSAGE_TYPE = "chat-v2-card";
export const CARD_PREVIEW_TEXT = "Sample card";

/** A `Custom` body whose payload is the message counter, as UTF-8 text. */
export function customBody(counter: number): ChatMessageBody {
  return {
    tag: "Custom",
    value: { messageType: CARD_MESSAGE_TYPE, payload: utf8ToHex(pad(counter, 4)), previewText: CARD_PREVIEW_TEXT },
  };
}

export const LEGACY_TEXT = "Legacy message from post_message";
export const DEEPLINK_TEXT = "Long-press this message to open the App";

// ---------------------------------------------------------------- history

/** The authors of every fifth history message, in turn. */
const HISTORY_AUTHORS = ["p1", "p2", "p3", "p4"];

/**
 * Message `index` (from 1) of `count` generated history messages. Every
 * fifth is by `p1` to `p4` in turn and the rest by the product. Every 25th
 * replies to the message 10 before it. Every 50th has a reaction of `p1`.
 */
export function historyMessage(index: number, count: number): StoredMessage {
  const author = index % 5 === 0 ? HISTORY_AUTHORS[(index / 5 - 1) % HISTORY_AUTHORS.length] : "product";
  return {
    message: {
      messageId: historyId(index),
      origin: { participantId: author },
      timestamp: historyTimestamp(index, count),
      body: textBody(`History message ${pad(index, 5)}`),
      status: "Sent",
      ...(index % 25 === 0 && index > 10 ? { replyTo: historyId(index - 10) } : {}),
      reactions: [],
    },
    reactions: index % 50 === 0 ? [{ participantId: "p1", emoji: "👍" }] : [],
  };
}
