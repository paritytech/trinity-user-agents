import { useEffect, useState } from 'react';

import { submitError$ } from './service';

/**
 * Subscribes to statement-store submit errors while `enabled`, resetting on
 * disable. Returns the raw domain `Error`; the consuming UI maps it to copy.
 */
export const useSubmitError = (enabled: boolean): Error | null => {
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    if (!enabled) {
      setError(null);

      return;
    }

    const subscription = submitError$.subscribe(setError);

    return () => subscription.unsubscribe();
  }, [enabled]);

  return error;
};
