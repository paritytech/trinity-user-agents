/**
 * The synthetic chat product's worker (`browser/synthetic-chat-product.ts`),
 * for the browser smoke test. The host's ordinary worker loader runs it as a
 * `Worker` execution, and it speaks the Chat Extension v2 draft bridge. What
 * it sees is on `window.__chatProduct` for the test to read.
 */
import { connectChatV2 } from "../src/chat-v2/product.js";
import { startSyntheticChatProduct } from "./synthetic-chat-product.js";

const product = startSyntheticChatProduct(connectChatV2(), { stepMs: 50 });
await product.ready;
(window as unknown as { __syntheticChat: unknown }).__syntheticChat = product;
