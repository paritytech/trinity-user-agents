import { createFileRoute, redirect } from '@tanstack/react-router';

import { truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';
import { OnboardingScreen } from '@/features/onboarding';

export const Route = createFileRoute('/_app/onboarding')({
  component: () => <OnboardingScreen />,
  // Block the onboarding mount until the core has reported auth at least once.
  // Without this an already-paired user briefly hits /onboarding on cold start
  // and the screen kicks off a spurious pairing.
  loader: async () => {
    const auth = await truapiRuntimeUseCase.whenAuthResolved();
    if (auth.tag === 'Connected') {
      redirect({ to: '/dashboard', throw: true });
    }
  },
});
