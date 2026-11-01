/**
 * What the request service (`invitation-service.ts`) and its network delivery
 * (`statement-delivery.ts`) report to the developer tools as it happens, for
 * the inspector's Statements view. Each event is a fact the host holds:
 * handles, accounts, indexes, periods and reasons. None carries a payload,
 * sealed bytes, a key, a name or what anyone wrote.
 */
export type InvitationEvent =
  /** The user sent a request; the host keeps it posted until it closes. */
  | { type: "request-sent"; product: string; handle: string }
  /** An authenticated request arrived for this user. */
  | { type: "request-received"; product: string; handle: string }
  /** An authenticated reply to a sent request reached the product. */
  | { type: "reply-authenticated"; product: string; handle: string }
  /** The product replied to a received request; the host posted the reply. */
  | { type: "replied"; product: string; handle: string }
  | {
      type: "request-closed";
      product: string;
      handle: string;
      how: "completed" | "cancelled";
    }
  /** The delivery cannot keep a sent request posted, or, with null, can again. */
  | {
      type: "request-paused";
      product: string;
      handle: string;
      problem: string | null;
    }
  /**
   * The product's `chatAccount` call for the conversation a request set up,
   * and how the host answered: an allowance it got now, one it kept from
   * earlier this period, or a refusal.
   */
  | {
      type: "chat-account";
      product: string;
      handle: string;
      /** The raw derivation index the product named, `0x` hex. */
      index: string;
      outcome: "allowed" | "reused" | "refused";
      account?: string;
      until?: number;
      reason?: string;
    }
  /** The delivery is about to post `post` (a request or reply handle) as this notification account. */
  | { type: "delivery-account"; post: string; account: string; period: number }
  /** The delivery cannot keep `post` up, or, with null, can again. */
  | { type: "delivery-problem"; post: string; problem: string | null };

export type InvitationObserver = (event: InvitationEvent) => void;
