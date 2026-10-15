/**
 * The cards of the host's own Pocket screen, as the native Pocket draws them.
 *
 * The native list holds two cards: the digital dollar, then Humanity. Humanity
 * is pinned by the host and drawn from a face bundled with the app until the
 * personhood product streams its own, so its face is fixed text, not a reading
 * of anything. The digital dollar shows the balance of the wallet's Coinage
 * coins, which belongs to an engine this host does not have yet.
 *
 * Nothing here reads a chain or invents a figure.
 */

/** The title the native top bar shows. `pocket_toolbar_title`. */
export const POCKET_TITLE = "Pocket";

/** What the digital dollar's amount slot holds. Only the unavailable state exists until a Coinage engine does. */
export type DigitalDollarAmount = { state: "unavailable"; reason: string };

export interface DigitalDollarCard {
  kind: "digital-dollar";
  id: "digital_dollar_card";
  amount: DigitalDollarAmount;
}

/** A card the host places for a product. The face inside is the product's own. */
export interface ProductCard {
  kind: "product";
  id: "humanity";
  title: string;
  subtitle: string;
  /** A pinned card is never offered for removal. */
  pinned: boolean;
}

export type PocketCard = DigitalDollarCard | ProductCard;

/** Why the amount slot reads "Unavailable". */
export const COINAGE_MISSING =
  "The digital dollar balance comes from a Coinage engine that holds the wallet's coins. This web host does not have one yet, so no amount is shown.";

/**
 * Humanity's bundled face: `assets/pocket/humanity.json`, a title and a
 * subtitle. `pocket_pinned_card_humanity` is the title.
 */
const HUMANITY: ProductCard = {
  kind: "product",
  id: "humanity",
  title: "Humanity",
  subtitle: "Proof of personhood",
  pinned: true,
};

/** The cards in the order the native list stacks them: the balance first, then the products. */
export function pocketCards(): PocketCard[] {
  return [
    {
      kind: "digital-dollar",
      id: "digital_dollar_card",
      amount: { state: "unavailable", reason: COINAGE_MISSING },
    },
    HUMANITY,
  ];
}
