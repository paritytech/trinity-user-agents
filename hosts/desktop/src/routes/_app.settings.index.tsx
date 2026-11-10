import { createFileRoute, redirect } from '@tanstack/react-router';

export const Route = createFileRoute('/_app/settings/')({
  loader: () => {
    redirect({
      to: '/settings/appearance',
      throw: true,
    });
  },
});
