/**
 * Products Chat SDK - Welcome Message
 *
 * Sends a welcome message exactly once per chat, using host localStorage
 * to track whether it has already been sent.
 */

import { hostLocalStorage } from '@novasamatech/host-api-wrapper';
import type { ChatMessageContent } from '@novasamatech/host-api-wrapper';

function welcomeSentKey(roomId: string): string {
    return `welcome_sent_${roomId}`;
}

type ChatManager = {
    sendMessage(roomId: string, payload: ChatMessageContent): Promise<{ messageId: string }>;
};

/**
 * Send a welcome message if it hasn't been sent yet.
 * Uses host localStorage to persist a "sent" flag so the message
 * is only delivered once across app restarts.
 *
 * @param chat - The product chat manager
 * @param roomId - The room to send the message to
 * @param payload - The message content (e.g., enumValue('Text', '...'))
 */
export async function sendWelcomeMessage(
    chat: ChatManager,
    roomId: string,
    payload: ChatMessageContent,
): Promise<void> {
    const key = welcomeSentKey(roomId);
    const alreadySent = await hostLocalStorage.readString(key);
    if (alreadySent) return;

    await chat.sendMessage(roomId, payload);
    await hostLocalStorage.writeString(key, 'true');
}
