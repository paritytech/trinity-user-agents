/**
 * The command table and the command parser. The Worker runs the commands and
 * the App shows the same table, so this module imports no client code.
 */
import type { ChatCommand, ChatCommandSpec } from "@parity/truapi";

/** The rooms where a command has a meaning. */
export type CommandRooms = "both" | "host" | "source";

export interface CommandDefinition {
  /** The name, without the `/`. */
  name: string;
  /** The hint for the arguments; empty for a command without arguments. */
  args: string;
  /** One line for the catalogue and for `/help`. */
  description: string;
  /** Where the command runs. */
  rooms: CommandRooms;
  /**
   * `true`: the command posts messages of its own, so it posts no
   * `ok <command>` reply.
   */
  posts: boolean;
  /** The scenarios of the testing plan that the command drives. */
  scenarios: string;
}

export const COMMANDS: readonly CommandDefinition[] = [
  { name: "help", args: "", description: "List every command", rooms: "both", posts: true, scenarios: "S7" },
  { name: "meta", args: "<name>", description: "Rename the room, change its avatar and description", rooms: "both", posts: false, scenarios: "S3" },
  { name: "preview", args: "pin|unpin|badge <n>|image|none|typing on|off|text <t>", description: "Change how the room shows in the chat list", rooms: "both", posts: false, scenarios: "S4" },
  { name: "room", args: "create|delete", description: "Create or delete the throwaway room", rooms: "both", posts: false, scenarios: "S5" },
  { name: "post", args: "text|rich|video|file|actions|custom|legacy|all", description: "Post one message of each body kind", rooms: "both", posts: true, scenarios: "S1, S7, S13, S48" },
  { name: "stream", args: "", description: "Stream one message word by word", rooms: "both", posts: true, scenarios: "S8" },
  { name: "flood", args: "[n]", description: "Send n edits at once and count the refused ones", rooms: "both", posts: true, scenarios: "S44" },
  { name: "edit", args: "<id> <text>", description: "Edit a message", rooms: "both", posts: false, scenarios: "S8" },
  { name: "delete", args: "<id>", description: "Delete a message", rooms: "both", posts: false, scenarios: "S9" },
  { name: "react", args: "<id> <emoji>", description: "Place a reaction of p1", rooms: "both", posts: false, scenarios: "S10" },
  { name: "unreact", args: "<id> <emoji>", description: "Remove a reaction of p1", rooms: "both", posts: false, scenarios: "S10" },
  { name: "status", args: "<id> sent|delivered|read|failed", description: "Change the status of a message", rooms: "both", posts: false, scenarios: "S11" },
  { name: "menu", args: "custom|extend|hide-edit|none", description: "Set the message menu", rooms: "both", posts: false, scenarios: "S25, S26, S28" },
  { name: "toolbar", args: "<n>", description: "Set a toolbar of n buttons", rooms: "both", posts: false, scenarios: "S29" },
  { name: "participants", args: "<n>", description: "Declare p1 to p4 and n guests", rooms: "both", posts: false, scenarios: "S30, S31, B1" },
  { name: "reactions", args: "on|na|off", description: "Set who-reacted details", rooms: "both", posts: false, scenarios: "S32" },
  { name: "footer", args: "input|photo|empty|custom|pay", description: "Set the footer", rooms: "both", posts: false, scenarios: "S33 to S36" },
  { name: "placeholder", args: "[text]", description: "Set the empty-room text", rooms: "both", posts: false, scenarios: "S37" },
  { name: "suggest", args: "[off]", description: "Show or remove the quick replies", rooms: "both", posts: false, scenarios: "S38" },
  { name: "commands", args: "[off]", description: "Declare or remove the command catalogue", rooms: "both", posts: false, scenarios: "S14, S39" },
  { name: "activity", args: "on|off", description: "Show or clear the activity line", rooms: "both", posts: false, scenarios: "S40" },
  { name: "allowed", args: "[emoji]", description: "Limit the reaction picker", rooms: "both", posts: false, scenarios: "S41" },
  { name: "caps", args: "", description: "Post the Chat features this host serves", rooms: "both", posts: true, scenarios: "S49, S50" },
  { name: "features", args: "preimage|search|readaloud on|off, calls audio|video|both|off", description: "Declare one optional feature of the room", rooms: "both", posts: false, scenarios: "S51" },
  { name: "history", args: "<n>", description: "Replace the messages with n generated ones", rooms: "source", posts: true, scenarios: "S15 to S17, S22, B2" },
  { name: "slow", args: "<ms>", description: "Delay every later answer", rooms: "both", posts: false, scenarios: "S21, S46" },
  { name: "fail", args: "reject|timeout|crash|unavailable", description: "Make the next answer fail", rooms: "both", posts: false, scenarios: "S20, S21, S23, S32, S46" },
  { name: "deeplink", args: "", description: "Post a message whose menu opens the App", rooms: "both", posts: true, scenarios: "S27" },
  { name: "echo", args: "", description: "Echo every user event", rooms: "both", posts: false, scenarios: "S12 to S14" },
];

const BY_NAME = new Map(COMMANDS.map((command) => [command.name, command]));

export function findCommand(name: string): CommandDefinition | undefined {
  return BY_NAME.get(name);
}

/** A command as the user typed it. */
export interface ParsedCommand {
  /** The name, without the `/`, in lower case. */
  name: string;
  /** The words after the name. */
  args: string[];
  /** The text after the name, with its inner spaces kept. */
  rest: string;
  /** The command line, normalized: the name and the words with one space. */
  line: string;
}

/**
 * Parse a text whose first word starts with `/`. Any other text gives
 * `undefined`.
 */
export function parseCommand(text: string): ParsedCommand | undefined {
  const trimmed = text.trim();
  if (!trimmed.startsWith("/")) return undefined;
  const match = /^\/(\S*)\s*([\s\S]*)$/.exec(trimmed);
  if (!match || match[1] === "") return undefined;
  const name = match[1].toLowerCase();
  const rest = match[2].trim();
  const args = rest === "" ? [] : rest.split(/\s+/);
  return { name, args, rest, line: ["/" + name, ...args].join(" ") };
}

/** Parse a `ChatEvent::Command`: the name, with or without `/`, and its payload. */
export function parseCommandEvent(command: ChatCommand): ParsedCommand | undefined {
  const name = command.command.trim().replace(/^\//, "");
  return parseCommand(`/${name} ${command.payload}`);
}

/** The catalogue `/commands` declares. */
export function commandSpecs(): ChatCommandSpec[] {
  return COMMANDS.map(({ name, args, description }) =>
    args === "" ? { name, description } : { name, description, argumentHint: args },
  );
}

/** The Markdown of `/help`: one list entry per command. */
export function helpMarkdown(): string {
  const lines = COMMANDS.map(({ name, args, description, rooms }) => {
    const hint = args === "" ? "" : ` \`${args}\``;
    const where = rooms === "both" ? "" : ` (${rooms} only)`;
    return `- **/${name}**${hint}: ${description}${where}`;
  });
  return ["**Chat Extension v2 sample commands**", "", ...lines].join("\n");
}
