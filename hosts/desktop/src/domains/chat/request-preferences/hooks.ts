import { useAction, useRead } from '@/shared/hooks';

import { HIDE_REQUESTS_BY_DEFAULT } from './constants';
import { hideRequestsByDefaultResource, setHideRequestsByDefault } from './resource';

const NO_PARAMS = {};

/**
 * Whether incoming message-request content is hidden by default. Drives the
 * "Message hidden based on your default settings" placeholder; a per-request
 * `revealed` flag overrides it once the user taps "View message".
 */
export const useHideRequestsByDefault = () => {
  return useRead(hideRequestsByDefaultResource, {
    params: NO_PARAMS,
    defaultValue: HIDE_REQUESTS_BY_DEFAULT,
  });
};

export const useSetHideRequestsByDefault = () => useAction(setHideRequestsByDefault);
