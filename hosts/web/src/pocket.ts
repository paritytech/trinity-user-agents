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

import type { PocketPlatform } from "@parity/truapi-host";
import type { PocketCardKey } from "./pocket-faces.js";
import { constant } from "./subscription.js";

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
  /**
   * The label of the product that backs the card. With the network's TLD it
   * is the product id, such as `peopl.paseo`.
   */
  product: string;
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
 * subtitle. `pocket_pinned_card_humanity` is the title. The personhood
 * product, the governance-reserved `peopl`, backs it, as it does in the
 * native apps.
 */
const HUMANITY: ProductCard = {
  kind: "product",
  id: "humanity",
  product: "peopl",
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

/** `card` as Pocket keys it on the network whose TLD is `networkSuffix`. */
export function cardKey(
  card: ProductCard,
  networkSuffix: string,
): PocketCardKey {
  return { productId: `${card.product}.${networkSuffix}`, cardId: card.id };
}

/** The pinned product cards on the network whose TLD is `networkSuffix`. */
export function pinnedCards(networkSuffix: string): PocketCardKey[] {
  return pocketCards().flatMap((card) =>
    card.kind === "product" && card.pinned
      ? [cardKey(card, networkSuffix)]
      : [],
  );
}

/**
 * Where an opened card starts its product's Widget: the card named in the
 * query, as every host builds it.
 */
export function cardLaunchPath(cardId: string): string {
  return `/?card=${encodeURIComponent(cardId)}`;
}

/**
 * The Pocket a product sees: the cards it backs. They are the pinned ones,
 * which no product can remove. The core's web bridge carries a refusal as
 * `Unknown` with this reason, not as `Privileged`.
 */
export function pocketPlatform(
  networkSuffix: string,
): Required<PocketPlatform> {
  const cardsOf = (productId: string) =>
    pinnedCards(networkSuffix).filter((card) => card.productId === productId);
  return {
    subscribePocketCards: (product) =>
      constant({
        cards: cardsOf(product.productId).map(({ cardId }) => ({
          cardId,
          privileged: true,
        })),
      }),
    removePocketCard: (product, request) =>
      cardsOf(product.productId).some((card) => card.cardId === request.cardId)
        ? Promise.reject(
            new Error(
              `Privileged: ${request.cardId} is pinned by the host and cannot be removed.`,
            ),
          )
        : Promise.resolve(),
  };
}
