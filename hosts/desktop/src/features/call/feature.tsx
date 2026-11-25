import { createFeature } from '@/shared/feature';
import { $features } from '@/shared/feature-config';
import { persistentSlot } from '@/features/app-shell';
import { canPlaceCallAnyOf, chatRoomBannerSlot, chatRoomHeaderActionsSlot } from '@/features/chat';

import { BridgeEndpointBinding } from './ui/BridgeEndpointBinding';
import { CallActivityBar } from './ui/CallActivityBar';
import { CallButtons } from './ui/CallButtons';
import { IncomingCallDetector } from './ui/IncomingCallDetector';

// Runtime feature flag. The feature's slot handlers report available only while
// `calls` is enabled in the feature-config store, so toggling it at runtime
// (window.__browser_config.enableFeature('calls')) mounts/unmounts the whole call
// UI — buttons, incoming-call detector, bridge wiring — without a rebuild.
export const callFeature = createFeature({ name: 'chat/call', enable: $features.map(features => features.calls) });

callFeature.inject(chatRoomHeaderActionsSlot, ({ session }) => <CallButtons session={session} />);

callFeature.inject(chatRoomBannerSlot, ({ session }) => <CallActivityBar session={session} />);

callFeature.inject(persistentSlot, () => <IncomingCallDetector />);

callFeature.inject(persistentSlot, () => <BridgeEndpointBinding />);

// Answers chat's capability probe. `createFeature` reports every handler this feature
// injects as unavailable while the `calls` flag is off, so chat receives `false` without
// ever learning that the flag exists.
callFeature.inject(canPlaceCallAnyOf, () => true);
