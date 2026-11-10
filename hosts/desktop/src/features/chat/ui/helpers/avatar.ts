import { type AvatarSize as TrUiAvatarSize, type AvatarTone, avatarTones } from '@novasamatech/tr-ui';

// Semantic sizes used across the chat UI, mapped to trUI's fixed pixel sizes. `chat-list` was 64px
// (no trUI equivalent) → nearest 56.
export type AvatarSize = 'tiny' | 'medium' | 'big' | 'chat-header' | 'chat-list' | 'chat-list-compact';

export const avatarSizeMap: Record<AvatarSize, TrUiAvatarSize> = {
  tiny: '20',
  'chat-header': '40',
  medium: '48',
  'chat-list-compact': '48',
  big: '56',
  'chat-list': '56',
};

// Stable name → tone so the same contact always gets the same trUI avatar color.
export function getAvatarTone(name: string): AvatarTone {
  const hash = name.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
  return avatarTones[hash % avatarTones.length] ?? 'amethyst';
}
