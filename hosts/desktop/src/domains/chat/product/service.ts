import { blake2b } from '@noble/hashes/blake2.js';
import { toHex } from '@novasamatech/scale';

import { type ProductChatRoom } from './types';

function belongsToProduct(room: ProductChatRoom, productId: string) {
  return room.productId === productId;
}

function getSessionId(productId: string, roomId: string, userId: string) {
  return toHex(blake2b(new TextEncoder().encode(`${userId}-${productId}-${roomId}`)));
}

export const productChatService = {
  belongsToProduct,
  getSessionId,
};
