import { createFileRoute } from '@tanstack/react-router';

import { ChatSettingsPage } from '@/features/chat';

export const Route = createFileRoute('/_app/settings/chats')({
  component: ChatSettingsPage,
});
