import { describe, expect, test } from "bun:test";
import { MemoryStorage } from "../../../support/test-support.js";
import { blockedAccounts, chatRecordKey } from "../../../../src/chat/store.js";

const contact = (account: string, blocked?: boolean) => ({
  account,
  chatKey: `0x${"cc".repeat(32)}`,
  name: null,
  state: "active",
  messages: [],
  outbound: null,
  expiry: { request: null, response: null },
  acknowledged: null,
  ...(blocked === undefined ? {} : { blocked }),
});

describe("the chat record", () => {
  test("names the people blocked in it, and reading it writes nothing", () => {
    const storage = new MemoryStorage();
    const key = chatRecordKey("paseo", "w1");
    expect(blockedAccounts(storage, key)).toEqual([]);
    storage.setItem(
      key,
      JSON.stringify({
        contacts: [
          contact(`0x${"aa".repeat(32)}`),
          contact(`0x${"bb".repeat(32)}`, true),
          contact(`0x${"dd".repeat(32)}`, false),
        ],
        requests: [],
      }),
    );
    expect(blockedAccounts(storage, key)).toEqual([`0x${"bb".repeat(32)}`]);

    storage.setItem(key, "not a record");
    expect(blockedAccounts(storage, key)).toEqual([]);
    expect(storage.getItem(`${key}.unreadable`)).toBeNull();
  });
});
