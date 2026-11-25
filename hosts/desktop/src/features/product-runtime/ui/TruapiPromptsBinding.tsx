import { useEffect } from 'react';

import { useDeviceBlockedPrompt } from '../hooks/useDeviceBlockedPrompt';
import { useHostPrompts } from '../hooks/useHostPrompts';
import { registerHostPrompts } from '../state/prompts';

/**
 * Lets the TrUAPI core ask the user. The core itself is booted by
 * `bootstrapProductRuntime`; asking means rendering a dialog into this tree, so the
 * prompts are registered while this is mounted and the core's prompt calls wait for
 * them. Renders nothing.
 */
export const TruapiPromptsBinding = () => {
  const prompts = useHostPrompts();
  useDeviceBlockedPrompt();

  useEffect(() => registerHostPrompts(prompts), [prompts]);

  return null;
};
