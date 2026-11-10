import { File, Image as ImageIcon, Phone, Video } from 'lucide-react';

import { cnTw } from '@/shared/utils';
import { type MessagePreviewIcon } from '../helpers/message';

type Props = {
  kind: MessagePreviewIcon;
  className?: string;
};

// `Phone` / `Video` deliberately match CallMessageBubble, so a call reads the
// same in the list as it does in the conversation.
export const PreviewIcon = ({ kind, className }: Props) => {
  if (!kind) return null;

  const Icon =
    kind === 'image' ? ImageIcon : kind === 'video' || kind === 'videoCall' ? Video : kind === 'voiceCall' ? Phone : File;

  return <Icon className={cnTw('size-4 shrink-0', className)} />;
};
