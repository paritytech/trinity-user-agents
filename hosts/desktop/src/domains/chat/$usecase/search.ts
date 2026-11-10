import { listAllP2PMessages } from '../p2p/repository';
import { listAllProductMessages } from '../product/repository';
import { chatMessageService } from '../session/service';
import { type ChatMessage } from '../session/types';

// Cap results so a single keystroke doesn't paint hundreds of rows; the user narrows to find older matches.
const MAX_RESULTS = 50;

// `text` is the matched message's current text (latest edit), so the row highlights what the conversation shows.
export type MessageSearchHit = { message: ChatMessage; text: string };

// Fold `edit` rows into their targets so search resolves current text, matching Room's in-room search.
function latestEditTextByMessageId(messages: ChatMessage[]): Map<string, string> {
  const latest = new Map<string, { text: string; timestamp: number }>();
  for (const message of messages) {
    if (message.content.type !== 'edit') continue;
    const targetId = message.content.messageId;
    const text = message.content.newContent.text ?? '';
    const prev = latest.get(targetId);
    if (!prev || message.timestamp > prev.timestamp) latest.set(targetId, { text, timestamp: message.timestamp });
  }

  const resolved = new Map<string, string>();
  for (const [id, entry] of latest) resolved.set(id, entry.text);

  return resolved;
}

/**
 * Case-insensitive substring search over every persisted chat message, newest-first. Composes the P2P and
 * product repositories (why it's a use case, not a single-module resource); `edit` rows are folded into their
 * originals so every hit maps to an in-room-locatable message id.
 */
async function searchMessages(query: string): Promise<MessageSearchHit[]> {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];

  const [p2pMessages, productMessages] = await Promise.all([listAllP2PMessages(), listAllProductMessages()]);
  const all = [...p2pMessages, ...productMessages];
  const latestEdits = latestEditTextByMessageId(all);

  const hits: MessageSearchHit[] = [];
  for (const message of all) {
    if (message.content.type === 'edit') continue;
    const text = latestEdits.get(message.messageId) ?? chatMessageService.getSearchableText(message.content);
    if (text.toLowerCase().includes(needle)) hits.push({ message, text });
  }

  return hits.sort((a, b) => b.message.timestamp - a.message.timestamp).slice(0, MAX_RESULTS);
}

export const searchUseCase = { searchMessages };
