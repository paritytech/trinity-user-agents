import { type AccountId } from '@/domains/network';
import { type RoomUserInput } from '../session/types';

export type ProductChatRoom = {
  sessionId: string;
  roomId: string;
  productId: string;
  userId: AccountId;
  createdAt: number;
  userInput?: RoomUserInput;
};
