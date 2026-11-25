import { useEffect, useRef } from 'react';
import { useObservable } from 'react-rx';

import { isElectron } from '@/shared/env';
import { type ChatSession, callService } from '@/domains/chat';
import { useP2PSessions } from '@/aggregates/p2p-chat';
import { stashIncomingOffer } from '../state/incomingOffers';

// Track which offerIds have already triggered a call window in this session
// so we don't re-open if the messages observable re-emits.
const handledOfferIds = new Set<string>();

export const IncomingCallDetector = () => {
  // Calls are P2P-only; product sessions don't ring.
  const { data: sessions } = useP2PSessions();

  return (
    <>
      {sessions.map(session => (
        <SessionCallWatcher key={session.sessionId} session={session} />
      ))}
    </>
  );
};

// Per-session headless watcher.
const SessionCallWatcher = ({ session }: { session: ChatSession }) => {
  const messages = useObservable(session.messages, []);
  const peerName = useObservable(session.name, '');
  // Keep stable ref for use in effects
  const peerNameRef = useRef(peerName);
  useEffect(() => {
    peerNameRef.current = peerName;
  }, [peerName]);

  useEffect(() => {
    if (!isElectron()) return;
    const ringable = callService.ringableIncomingOffers(messages);
    for (const offer of ringable) {
      if (handledOfferIds.has(offer.messageId)) continue;
      handledOfferIds.add(offer.messageId);

      const purpose = callService.offerPurpose(offer) ?? 'audio';
      const offerSdp = offer.content.type === 'callSignal' ? offer.content.sdp : undefined;
      console.info(
        '[CALL] incoming offer detected: offerId=%s purpose=%s sdp=%d bytes',
        offer.messageId,
        purpose,
        offerSdp?.length ?? 0,
      );
      // Stash the offer SDP (encoded MinimalSetup) so the bridge endpoint can
      // ship it in provideIncomingCall once the call window's port arrives.
      if (offerSdp) {
        stashIncomingOffer(offer.messageId, offerSdp);
      }
      // The call window uses offer.messageId as its offerId; the bridge endpoint
      // maps it back to the chat messageId to route the answer.
      void window.App.openCallWindow(
        {
          direction: 'incoming',
          purpose,
          offerId: offer.messageId,
          peerName: peerNameRef.current,
          peerSessionId: session.sessionId,
        },
        {
          turnHost: import.meta.env['VITE_WEBRTC_TURN_HOST'],
          turnSecret: import.meta.env['VITE_WEBRTC_TURN_SECRET'],
        },
      ).then(() => {
        // Once the window is open, BridgeEndpointBinding receives the port from
        // main and sends provideIncomingCall.
      });
    }
  }, [messages]);

  return null;
};
