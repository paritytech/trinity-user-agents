import { describe, expect, test } from "bun:test";
import {
  COINAGE_MISSING,
  POCKET_TITLE,
  cardLaunchPath,
  pinnedCards,
  pocketCards,
  pocketPlatform,
} from "../../../src/pocket/pocket.js";

describe("pocketCards", () => {
  // The native list is the balance card first, then the pinned Humanity card.
  test("lists the digital dollar and then Humanity", () => {
    expect(pocketCards().map(({ id }) => id)).toEqual([
      "digital_dollar_card",
      "humanity",
    ]);
    expect(POCKET_TITLE).toBe("Pocket");
  });

  // No Coinage engine exists in this host, so any figure would be made up.
  test("gives the digital dollar no amount, and says why", () => {
    const [dollar] = pocketCards();
    expect(dollar).toEqual({
      kind: "digital-dollar",
      id: "digital_dollar_card",
      amount: { state: "unavailable", reason: COINAGE_MISSING },
    });
    expect(COINAGE_MISSING).not.toMatch(/\d/);
  });

  // The face is the bundled one, not a reading of the wallet's personhood.
  test("draws Humanity from its bundled face", () => {
    expect(pocketCards()[1]).toEqual({
      kind: "product",
      id: "humanity",
      product: "peopl",
      title: "Humanity",
      subtitle: "Proof of personhood",
      pinned: true,
    });
  });
});

describe("the pinned Humanity card", () => {
  // The native apps back it with the reserved personhood product of the
  // network they run on.
  test("belongs to peopl on the network's TLD", () => {
    expect(pinnedCards("paseo")).toEqual([
      { productId: "peopl.paseo", cardId: "humanity" },
    ]);
    expect(pinnedCards("testnet")).toEqual([
      { productId: "peopl.testnet", cardId: "humanity" },
    ]);
  });

  test("opens its Widget at the card named in the query", () => {
    expect(cardLaunchPath("humanity")).toBe("/?card=humanity");
    expect(cardLaunchPath("a b&c")).toBe("/?card=a%20b%26c");
  });

  // Each product sees only its own cards, and no product removes a pinned one.
  test("is the personhood product's only card, and cannot be removed", async () => {
    const pocket = pocketPlatform("paseo");
    const first = async (productId: string) => {
      for await (const item of pocket.subscribePocketCards({
        productId,
        executionKind: "Worker",
      }))
        return item._unsafeUnwrap();
    };
    expect(await first("peopl.paseo")).toEqual({
      cards: [{ cardId: "humanity", privileged: true }],
    });
    expect(await first("other.paseo")).toEqual({ cards: [] });
    await expect(
      pocket.removePocketCard(
        { productId: "peopl.paseo", executionKind: "Worker" },
        { cardId: "humanity" },
      ),
    ).rejects.toThrow("Privileged");
    // Removing a card that is not there succeeds.
    await pocket.removePocketCard(
      { productId: "other.paseo", executionKind: "Worker" },
      { cardId: "humanity" },
    );
  });
});
