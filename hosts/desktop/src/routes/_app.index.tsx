import { createFileRoute, redirect } from '@tanstack/react-router';

import { truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';

const Index = () => {
  return null;
};

export const Route = createFileRoute('/_app/')({
  component: Index,
  // Await the core's first auth emission before deciding the destination.
  // `null` is "the worker has not answered yet", not "signed out" — deciding on
  // it bounces an already-paired user through /onboarding on every cold start.
  loader: async () => {
    const auth = await truapiRuntimeUseCase.whenAuthResolved();
    redirect({
      to: auth.tag === 'Connected' ? '/dashboard' : '/onboarding',
      throw: true,
    });
  },
});
