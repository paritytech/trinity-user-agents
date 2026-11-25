import { type ChatMessage, type MessageContent } from '@/domains/chat';

import { callTitleKey, deriveCallStates } from './callState';

const truncateName = (name: string) => (name.length > 16 ? `${name.slice(0, 8)}...` : name);

export function getPlainText(content: MessageContent): string {
  switch (content.type) {
    case 'text':
      return content.text;
    case 'richText':
      return content.text ?? '';
    case 'reply':
      return getPlainText(content.content);
    default:
      return '';
  }
}

export function getMessagePreview(message: ChatMessage) {
  switch (message.content.type) {
    case 'text':
      return message.content.text;
    case 'contactAdded':
      return 'Accepted the request';
    case 'leftChat':
      return `${truncateName(message.peer.name)} left the chat`;
    case 'reacted':
      return `${truncateName(message.peer.name)} reacted to your message`;
    case 'reactionRemoved':
      return `${truncateName(message.peer.name)} removed a reaction`;
    case 'reply':
      return message.content.content.type === 'text' ? message.content.content.text : 'Replied to message';
    case 'richText': {
      if (message.content.text) return message.content.text;
      const attachment = message.content.attachments?.[0];
      if (!attachment) return '';
      if (attachment.meta.type === 'image') return 'Photo';
      if (attachment.meta.type === 'video') return 'Video';
      return 'File';
    }
    case 'edit':
      return message.content.newContent.text ?? '';
    case 'custom':
      return message.content.alt ?? 'Message';
    case 'transfer':
      return message.status.direction === 'outgoing' ? 'Sent funds' : 'Received funds';
    case 'callSignal':
      if (message.content.signal !== 'offer') return '';
      return message.content.purpose === 'video' ? 'Video call' : 'Voice call';
    default:
      return '';
  }
}

export type MessagePreviewIcon = 'image' | 'video' | 'file' | 'voiceCall' | 'videoCall' | null;

export function getMessagePreviewIcon(message: ChatMessage): MessagePreviewIcon {
  if (message.content.type === 'callSignal') {
    if (message.content.signal !== 'offer') return null;

    return message.content.purpose === 'video' ? 'videoCall' : 'voiceCall';
  }

  if (message.content.type !== 'richText') return null;
  const attachment = message.content.attachments?.[0];
  if (!attachment) return null;
  if (attachment.meta.type === 'image') return 'image';
  if (attachment.meta.type === 'video') return 'video';

  return 'file';
}

export type MessagePreview = {
  text: string;
  icon: MessagePreviewIcon;
};

export const EMPTY_PREVIEW: MessagePreview = { text: '', icon: null };

/**
 * The preview for a message, resolving a call to the same state-aware title
 * `CallMessageBubble` renders. `messages` is the room's full message list —
 * the call state is derived from the offer's answer/closed signals.
 */
export function getPreview(message: ChatMessage, messages: ChatMessage[], t: (key: string) => string): MessagePreview {
  const icon = getMessagePreviewIcon(message);
  if (message.content.type !== 'callSignal') return { text: getMessagePreview(message), icon };

  const state = deriveCallStates(messages).get(message.messageId);
  if (!state) return { text: getMessagePreview(message), icon };

  return { text: t(callTitleKey(state, message.status.direction === 'outgoing', message.content.purpose === 'video')), icon };
}

export type LatestEdit = {
  text: string;
  timestamp: number;
};

export function deriveLatestEdits(messages: ChatMessage[]): Map<string, LatestEdit> {
  const edits = new Map<string, LatestEdit>();

  for (const msg of messages) {
    if (msg.content.type !== 'edit') continue;

    const targetId = msg.content.messageId;
    const existing = edits.get(targetId);

    if (!existing || msg.timestamp > existing.timestamp) {
      edits.set(targetId, {
        text: msg.content.newContent.text ?? '',
        timestamp: msg.timestamp,
      });
    }
  }

  return edits;
}

export type EditHistoryEntry = {
  text: string;
  timestamp: number;
};

export function getEditHistory(messages: ChatMessage[], targetMessageId: string): EditHistoryEntry[] {
  const entries: EditHistoryEntry[] = [];

  for (const msg of messages) {
    if (msg.content.type !== 'edit') continue;
    if (msg.content.messageId !== targetMessageId) continue;

    entries.push({
      text: msg.content.newContent.text ?? '',
      timestamp: msg.timestamp,
    });
  }

  return entries.sort((a, b) => b.timestamp - a.timestamp);
}
