import { blake2b } from "@noble/hashes/blake2.js";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChainProvider } from "@parity/truapi-host";
import {
  CONSUMERS_PREFIX,
  decodeConsumer,
  mapKey,
  readStorageValues,
} from "../people-chain.js";
import {
  Reader,
  bytes,
  equalBytes,
  readBytes,
  readOption,
  utf8,
} from "./codec.js";

/**
 * Read-only lookups the chat needs from chain, at the newest finalized block.
 *
 * Asset Hub `DotnsGateway.AccountNames[account]` holds an account's lite and
 * full labels, each with an optional 65-byte chat key container
 * (`0x00 || x25519 || 32 bytes padding`), and `DotnsGateway.LiteLabelOwner`
 * maps a lite label back to its account. The Polkadot app reads a peer's chat
 * key from the full label's container, else the lite one's.
 *
 * People `Resources.Consumers[account]` holds the same 65-byte container as
 * its `identifier_key`, with the lite and full usernames, and
 * `Resources.UsernameOwnerOf` maps a username back to its account. Polkadot
 * app builds that register a username through the People chain write only
 * these, so an account can have a chat key there and no Asset Hub record.
 * A chat key is taken from either place, and refused when the two disagree.
 *
 * People `:statement_allowance:` keyed by account says whether that account
 * may write statements at all.
 */

/** `twox128("DotnsGateway") ++ twox128("AccountNames")`. */
const ACCOUNT_NAMES_PREFIX =
  "57ca9018d648b40a8286a471e82c3808de072516e985ec03dec70088d3f16ce3";
/** `twox128("DotnsGateway") ++ twox128("LiteLabelOwner")`. */
const LITE_LABEL_OWNER_PREFIX =
  "57ca9018d648b40a8286a471e82c3808fd6d83e29558d6c9f6558f7b19f9b5f1";

/** `twox128("Resources") ++ twox128("UsernameOwnerOf")`. */
const USERNAME_OWNER_OF_PREFIX =
  "2111e0df19de9563b58301e5f7e00743099c711e270af23bb9a4e46759a9761d";

const CHAT_KEY_CONTAINER_BYTES = 65;
const X25519_KEY_TYPE = 0;

/** A `Blake2_128Concat` map key over the already encoded `key`. */
const blake2Concat = (prefix: string, key: Uint8Array): HexString =>
  `0x${prefix}${bytesToHex(blake2b(key, { dkLen: 16 })).slice(2)}${bytesToHex(key).slice(2)}`;

export const accountNamesKey = (account: Uint8Array): HexString =>
  blake2Concat(ACCOUNT_NAMES_PREFIX, account);

export const liteLabelOwnerKey = (label: string): HexString =>
  blake2Concat(LITE_LABEL_OWNER_PREFIX, bytes(utf8(label)));

export const consumerKey = (account: Uint8Array): HexString =>
  mapKey(CONSUMERS_PREFIX, account);

export const usernameOwnerKey = (username: string): HexString =>
  blake2Concat(USERNAME_OWNER_OF_PREFIX, bytes(utf8(username)));

export const allowanceKey = (account: Uint8Array): HexString =>
  bytesToHex(new Uint8Array([...utf8(":statement_allowance:"), ...account]));

export interface ChainName {
  label: string;
  /** The X25519 chat key, or null when the label has none or an unknown key type. */
  chatKey: Uint8Array | null;
}

export interface AccountNames {
  lite: ChainName | null;
  full: ChainName | null;
}

/** The X25519 key in a 65-byte chat key container, or null for another key type. */
function containerKey(container: Uint8Array): Uint8Array | null {
  return container[0] === X25519_KEY_TYPE ? container.slice(1, 33) : null;
}

function readName(reader: Reader): ChainName {
  const label = new TextDecoder("utf-8", { fatal: true }).decode(
    readBytes(reader),
  );
  const chatKey = readOption(reader, (inner) =>
    containerKey(inner.take(CHAT_KEY_CONTAINER_BYTES)),
  );
  return { label, chatKey };
}

export function decodeAccountNames(encoded: Uint8Array): AccountNames {
  const reader = new Reader(encoded);
  const lite = readOption(reader, readName);
  const full = readOption(reader, readName);
  reader.finish();
  return { lite, full };
}

/** The chat key the app would use: the full label's, else the lite label's. */
export const chatKeyOf = (names: AccountNames): Uint8Array | null =>
  names.full?.chatKey ?? names.lite?.chatKey ?? null;

/** The name to show: the full label, else the lite one. */
export const labelOf = (names: AccountNames): string | null =>
  names.full?.label ?? names.lite?.label ?? null;

/** An account's People `Resources.Consumers` record, as chat needs it. */
export interface PeopleConsumer {
  liteUsername: string;
  fullUsername: string | null;
  /** The X25519 key of `identifier_key`, or null for another key type. */
  chatKey: Uint8Array | null;
}

export function decodePeopleConsumer(encoded: Uint8Array): PeopleConsumer {
  const { liteUsername, fullUsername } = decodeConsumer(encoded);
  return {
    liteUsername,
    fullUsername,
    chatKey: containerKey(encoded.subarray(0, CHAT_KEY_CONTAINER_BYTES)),
  };
}

/** What chat may use of an account's on-chain identity. */
export interface ChatIdentity {
  name: string | null;
  /** Null when neither chain names an X25519 key, or when they name different ones. */
  chatKey: Uint8Array | null;
  /** Both chains name an X25519 key, and they differ. */
  conflict: boolean;
}

/**
 * Combine an account's Asset Hub and People records. A key named by either is
 * used; two different keys are a conflict, and neither is used.
 */
export function resolveChatIdentity(
  names: AccountNames | null,
  consumer: PeopleConsumer | null,
): ChatIdentity | null {
  if (names === null && consumer === null) return null;
  const assetHubKey = names === null ? null : chatKeyOf(names);
  const peopleKey = consumer?.chatKey ?? null;
  const conflict =
    assetHubKey !== null &&
    peopleKey !== null &&
    !equalBytes(assetHubKey, peopleKey);
  return {
    name:
      (names === null ? null : labelOf(names)) ??
      consumer?.fullUsername ??
      consumer?.liteUsername ??
      null,
    chatKey: conflict ? null : (assetHubKey ?? peopleKey),
    conflict,
  };
}

export interface ChainDirectory {
  /** Asset Hub `DotnsGateway.AccountNames[account]`. */
  names(account: Uint8Array): Promise<AccountNames | null>;
  /** People `Resources.Consumers[account]`. */
  consumer(account: Uint8Array): Promise<PeopleConsumer | null>;
  /** The account a username names on Asset Hub and on People, each null when it names none. */
  owners(
    label: string,
  ): Promise<{ assetHub: Uint8Array | null; people: Uint8Array | null }>;
  hasAllowance(account: Uint8Array): Promise<boolean>;
}

/** Both records of an account, combined. A failed read of either fails the lookup. */
export async function lookupChatIdentity(
  directory: ChainDirectory,
  account: Uint8Array,
): Promise<ChatIdentity | null> {
  const [names, consumer] = await Promise.all([
    directory.names(account),
    directory.consumer(account),
  ]);
  return resolveChatIdentity(names, consumer);
}

/** The account a username names. Throws when the two chains name different accounts. */
export async function lookupOwner(
  directory: ChainDirectory,
  label: string,
): Promise<Uint8Array | null> {
  const { assetHub, people } = await directory.owners(label);
  if (assetHub !== null && people !== null && !equalBytes(assetHub, people))
    throw new Error(
      `${label} names different accounts on Asset Hub and on the People chain.`,
    );
  return assetHub ?? people;
}

function asAccount(value: HexString | null): Uint8Array | null {
  if (value === null) return null;
  const account = hexToBytes(value);
  if (account.length !== 32) throw new Error("the owner is not an account");
  return account;
}

/** The directory over the host's light client. Each call opens and closes its own connection. */
export function chainDirectory(
  chain: ChainProvider,
  genesis: { assetHub: HexString; people: HexString },
): ChainDirectory {
  const read = async (
    genesisHash: HexString,
    key: HexString,
    chainName: string,
  ) => {
    const connection = await chain.connect(hexToBytes(genesisHash));
    const { values } = await readStorageValues(
      connection,
      [key],
      undefined,
      [],
      chainName,
    );
    return values[0];
  };
  return {
    async names(account) {
      const value = await read(
        genesis.assetHub,
        accountNamesKey(account),
        "Asset Hub",
      );
      return value === null ? null : decodeAccountNames(hexToBytes(value));
    },
    async consumer(account) {
      const value = await read(genesis.people, consumerKey(account), "People");
      return value === null ? null : decodePeopleConsumer(hexToBytes(value));
    },
    async owners(label) {
      const [assetHub, people] = await Promise.all([
        read(genesis.assetHub, liteLabelOwnerKey(label), "Asset Hub"),
        read(genesis.people, usernameOwnerKey(label), "People"),
      ]);
      return { assetHub: asAccount(assetHub), people: asAccount(people) };
    },
    async hasAllowance(account) {
      return (
        (await read(genesis.people, allowanceKey(account), "People")) !== null
      );
    },
  };
}
