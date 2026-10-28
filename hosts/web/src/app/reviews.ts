import { bytesToHex } from "@parity/truapi/scale";
import type { UserConfirmationReview } from "@parity/truapi-host";

/** One labelled line of a prompt. */
export interface PromptField {
  label: string;
  value: string;
  /** Rendered as a warning the user should read before deciding. */
  warning?: boolean;
  /** Rendered in a monospace block, for hex and structured values. */
  mono?: boolean;
}

/** What the prompt for a review shows. */
export interface ReviewDescription {
  title: string;
  /** The product that asked, as the core names it. */
  product: string;
  fields: PromptField[];
}

const TITLES: Record<UserConfirmationReview["tag"], string> = {
  SignPayload: "Sign a transaction payload",
  SignRaw: "Sign a message",
  StatementStoreProductSign: "Sign a Statement Store statement",
  CreateTransaction: "Create a transaction",
  AccountAlias: "Derive an account alias",
  CreateProof: "Create a ring proof",
  IdentityDisclosure: "Share your identity",
  ResourceAllocation: "Allocate resources",
  AccountAccess: "Give access to another product's account",
  SignVrf: "Sign a VRF transcript",
  ProductSubtree: "Share the product account",
};

/**
 * Describe a review for the prompt: the product that asked, any warning the
 * protocol requires the host to show, and the full request.
 */
export function describeReview(
  review: UserConfirmationReview,
): ReviewDescription {
  const fields: PromptField[] = [];
  if (review.tag === "SignRaw" && !review.value.value.watermarked) {
    fields.push({
      label: "Warning",
      value:
        "These bytes are not watermarked, so they could be a valid transaction.",
      warning: true,
    });
  }
  fields.push({
    label: "Request",
    value: formatValue(review.value),
    mono: true,
  });
  return {
    title: TITLES[review.tag],
    product: requestingProduct(review) ?? "a product the core does not name",
    fields,
  };
}

function requestingProduct(review: UserConfirmationReview): string | undefined {
  switch (review.tag) {
    case "SignPayload":
    case "SignRaw":
    case "CreateTransaction":
      return review.value.tag === "Product"
        ? review.value.value.callingProductId
        : undefined;
    case "StatementStoreProductSign":
    case "AccountAlias":
    case "CreateProof":
    case "ResourceAllocation":
    case "SignVrf":
      return review.value.callingProductId;
    case "IdentityDisclosure":
    case "ProductSubtree":
      return review.value.productId;
    case "AccountAccess":
      return review.value.requestingProductId;
  }
}

/** JSON with bytes as hex and bigints as decimal, both of which JSON lacks. */
export function formatValue(value: unknown): string {
  return JSON.stringify(
    value,
    (_key, item: unknown) => {
      if (typeof item === "bigint") return item.toString();
      if (item instanceof Uint8Array) return bytesToHex(item);
      return item;
    },
    2,
  );
}
