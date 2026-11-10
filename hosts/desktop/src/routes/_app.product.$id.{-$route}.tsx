import { createFileRoute } from '@tanstack/react-router';

export const Route = createFileRoute('/_app/product/$id/{-$route}')({
  component: () => null,
});
