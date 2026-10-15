import { describe, expect, test } from "bun:test";
import { COINAGE_MISSING, POCKET_TITLE, pocketCards } from "./pocket.js";

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
      title: "Humanity",
      subtitle: "Proof of personhood",
      pinned: true,
    });
  });
});
