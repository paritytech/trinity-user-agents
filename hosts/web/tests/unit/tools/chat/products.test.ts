import { describe, expect, test } from "bun:test";
import {
  chatProductsOn,
  kettleIroh,
  moveDeliveryOff,
  setKettleIroh,
  setChatProductOn,
  setNewChats,
} from "../../../../tools/chat/products.js";
import { MemoryStorage } from "../../../support/test-support.js";
import {
  BAD_RELAY_URL,
  irohOfParams,
  irohParams,
  relayUrlOf,
} from "../../../../apps/kettle-chat/route.js";
import { kettleChat } from "../../../../tools/chat/kettle.js";

describe("the chat products a tab runs", () => {
  test("Contact Chat alone by default; either, both or neither as the switches say, the one New chat starts in first", () => {
    const storage = new MemoryStorage();
    const seen = [chatProductsOn(storage)];
    setChatProductOn(storage, "kettle-chat", true);
    seen.push(chatProductsOn(storage));
    setNewChats(storage, "kettle-chat");
    seen.push(chatProductsOn(storage));
    setChatProductOn(storage, "contact-chat", false);
    seen.push(chatProductsOn(storage));
    setChatProductOn(storage, "kettle-chat", false);
    seen.push(chatProductsOn(storage));
    setChatProductOn(storage, "contact-chat", true);
    // New chat's product is off: the one that runs starts new chats.
    seen.push(chatProductsOn(storage));
    expect(seen).toEqual([
      ["contact-chat"],
      ["contact-chat", "kettle-chat"],
      ["kettle-chat", "contact-chat"],
      ["kettle-chat"],
      [],
      ["contact-chat"],
    ]);
  });

  test("a tab whose delivery was Off keeps running no chat product, with the default delivery", () => {
    const storage = new MemoryStorage();
    storage.setItem("truapi-web-signing-host:dev:wallet-delivery", "off");
    setChatProductOn(storage, "kettle-chat", true);
    expect([
      moveDeliveryOff(storage),
      chatProductsOn(storage),
      storage.getItem("truapi-web-signing-host:dev:wallet-delivery"),
      moveDeliveryOff(storage),
    ]).toEqual([true, [], null, false]);
    storage.setItem("truapi-web-signing-host:dev:wallet-delivery", "local");
    expect([moveDeliveryOff(storage), chatProductsOn(storage)]).toEqual([
      false,
      [],
    ]);
  });
});

describe("Kettle Chat's iroh setting in a tab", () => {
  test("off by default; on, the public test relays or one custom relay, kept until changed; a refused custom relay changes nothing", () => {
    const storage = new MemoryStorage();
    const seen = [kettleIroh(storage)];
    setKettleIroh(storage, { relays: "public" });
    seen.push(kettleIroh(storage));
    setKettleIroh(storage, {
      relays: "custom",
      relayUrl: " http://127.0.0.1:3340 ",
    });
    seen.push(kettleIroh(storage));
    expect(() =>
      setKettleIroh(storage, {
        relays: "custom",
        relayUrl: "ws://127.0.0.1:3340",
      }),
    ).toThrow(BAD_RELAY_URL);
    seen.push(kettleIroh(storage));
    setKettleIroh(storage, null);
    seen.push(kettleIroh(storage));
    expect(seen).toEqual([
      null,
      { relays: "public" },
      { relays: "custom", relayUrl: "http://127.0.0.1:3340/" },
      { relays: "custom", relayUrl: "http://127.0.0.1:3340/" },
      null,
    ]);
  });

  test("the setting reaches Kettle Chat's worker as its page's parameters, and nothing when off; other chat products get none", () => {
    const params = (iroh: Parameters<typeof kettleChat>[1]) =>
      kettleChat(async () => {}, iroh).workerParams;
    expect([
      params(null),
      params({ relays: "public" }),
      params({ relays: "custom", relayUrl: "http://127.0.0.1:3340/" }),
    ]).toEqual([
      undefined,
      { iroh: "public" },
      { "iroh-relay": "http://127.0.0.1:3340/" },
    ]);
  });
});

describe("the worker's iroh parameters", () => {
  test("read back what the host set; anything else runs none and is refused", () => {
    const read = (query: string) => irohOfParams(new URLSearchParams(query));
    const custom = {
      relays: "custom",
      relayUrl: "http://127.0.0.1:3340/",
    } as const;
    expect([
      read(""),
      read(new URLSearchParams(irohParams({ relays: "public" })).toString()),
      read(new URLSearchParams(irohParams(custom)).toString()),
      read("iroh=private"),
      read("iroh="),
      read(
        `iroh-relay=${encodeURIComponent("https://u:synthetic-3@relay.example")}`,
      ),
      read(
        `iroh-relay=${encodeURIComponent("https://relay.example/?t=synthetic-3")}`,
      ),
      // A custom relay goes first; a bad one is not replaced by the public relays.
      read(
        `iroh=public&iroh-relay=${encodeURIComponent("ws://relay.example")}`,
      ),
    ]).toEqual([
      { iroh: null, refused: false },
      { iroh: { relays: "public" }, refused: false },
      { iroh: custom, refused: false },
      { iroh: null, refused: true },
      { iroh: null, refused: true },
      { iroh: null, refused: true },
      { iroh: null, refused: true },
      { iroh: null, refused: true },
    ]);
  });
});

describe("a Kettle Chat relay URL", () => {
  // Synthetic stand-ins for a secret: none is a real credential.
  const SECRET = "synthetic-secret-1";
  const refused = [
    `https://user:${SECRET}@relay.example`,
    `https://${SECRET}@relay.example`,
    `https://user%3A${SECRET}@relay.example`,
    `https://relay.example/?token=${SECRET}`,
    `https://relay.example/%3Ftoken=${SECRET}`,
    `https://relay.example/#${SECRET}`,
    `https://relay.example/%23${SECRET}`,
    `https://relay.example/${SECRET}`,
    "https://relay.example/?",
    "https://relay.example/#",
    "ws://relay.example",
    "file:///relay",
    "relay.example",
    "",
  ];

  test("holds a scheme, a host and a port only: a user, password, path, query or fragment, plain or percent-encoded, is refused", () => {
    expect([
      relayUrlOf("http://127.0.0.1:3340"),
      relayUrlOf(" HTTPS://Relay.Example:443/ "),
      ...refused.map(relayUrlOf),
    ]).toEqual([
      "http://127.0.0.1:3340/",
      "https://relay.example/",
      ...refused.map(() => null),
    ]);
  });

  test("a refused relay is not stored, and the error does not repeat it", () => {
    const storage = new MemoryStorage();
    const messages = refused.map((value) => {
      try {
        setKettleIroh(storage, { relays: "custom", relayUrl: value });
        return "stored";
      } catch (error) {
        return (error as Error).message;
      }
    });
    expect([...new Set(messages)]).toEqual([BAD_RELAY_URL]);
    expect(kettleIroh(storage)).toBeNull();
  });

  test("a stored value that is not a plain relay URL is read as none", () => {
    const storage = new MemoryStorage();
    storage.setItem(
      "truapi-web-signing-host:dev:kettle-iroh-relay",
      `https://relay.example/?token=${SECRET}`,
    );
    expect(kettleIroh(storage)).toBeNull();
  });
});
