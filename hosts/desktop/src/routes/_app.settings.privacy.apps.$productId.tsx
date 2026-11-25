import { Outlet, createFileRoute } from '@tanstack/react-router';

export const Route = createFileRoute('/_app/settings/privacy/apps/$productId')({
  component: () => <Outlet />,
});
