import {
  ProductId,
  asProductId,
  type HostGetProductContextResponse,
} from "../src/generated/types.js";

const explicit: ProductId = asProductId("peopl.dot");
const decoded: ProductId = ProductId.dec(ProductId.enc(explicit));
const response: HostGetProductContextResponse = { productId: decoded };
const received: ProductId = response.productId;
const text: string = received;
void text;

// @ts-expect-error IDs must be deliberately designated or received from the host.
const implicit: ProductId = "peopl.dot";
void implicit;

// @ts-expect-error The codec must preserve ProductId identity at its input boundary.
ProductId.enc("peopl.dot");
