import { createFileRoute } from '@tanstack/react-router';

import { NetworkConnectionSettings } from '@/features/network-connection';

export const Route = createFileRoute('/_app/settings/network')({
  component: NetworkConnectionSettings,
});
