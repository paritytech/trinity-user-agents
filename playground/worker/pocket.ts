import type { HostPocketListSubscribeItem } from "@parity/truapi";

/** The Pocket surface of the client, as this worker uses it. */
interface PocketSurface {
  listSubscribe(): {
    subscribe(observer: {
      next?: (item: HostPocketListSubscribeItem) => void;
      error?: (reason: unknown) => void;
    }): unknown;
  };
}

/**
 * The collection as one line, marking the cards the host pinned: those are the
 * ones a product is refused when it asks to remove them.
 */
export function describePocketCards(item: HostPocketListSubscribeItem): string {
  if (item.cards.length === 0) {
    return "no cards";
  }
  return item.cards
    .map((card) => (card.privileged ? `${card.cardId} (pinned)` : card.cardId))
    .join(", ");
}

/**
 * Report the product's own card collection whenever the host replaces it.
 *
 * Removal is deliberately not wired: a card leaves Pocket because the user
 * asked or because the product decided to give it up, and a demo product that
 * dropped one on startup would take a card off the user's screen.
 */
export function servePocket(pocket: PocketSurface): void {
  pocket.listSubscribe().subscribe({
    next(item) {
      console.log("TrUAPI Playground Pocket cards:", describePocketCards(item));
    },
    error(reason) {
      console.error("TrUAPI Playground Pocket card list failed", reason);
    },
  });
}
