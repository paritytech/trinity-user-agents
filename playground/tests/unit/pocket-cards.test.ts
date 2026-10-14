import { describe, expect, test } from "bun:test";
import { describePocketCards } from "../../worker/pocket";

describe("describePocketCards", () => {
  test("marks the cards the host pinned, because those refuse removal", () => {
    const described = describePocketCards({
      cards: [
        { cardId: "loyalty", privileged: false },
        { cardId: "humanity", privileged: true },
      ],
    });

    expect(described).toBe("loyalty, humanity (pinned)");
  });

  test("says so when the product backs no card at all", () => {
    expect(describePocketCards({ cards: [] })).toBe("no cards");
  });
});
