import { useEffect, useRef } from 'react';
import { concatMap, pairwise } from 'rxjs';

import { type CallWindowLaunch } from '@/shared/call-bridge';
import { isElectron } from '@/shared/env';
import {
  type MainToWindowMessage,
  type OfferIdMap,
  type WindowToMainMessage,
  callService,
  parseWindowToMain,
} from '@/domains/chat';
import { useP2PSessions } from '@/aggregates/p2p-chat';
import { setCallActive, setCallIdle } from '../state/callActivity';
import { takeIncomingOffer } from '../state/incomingOffers';

/**
 * Headless component — returns null. Receives the MessagePort from main
 * (posted on 'call:bridge-port' after the call window opens) and wires
 * the window↔session relay.
 *
 * Assumes one active call: a single port, shared offerIdMap. Concurrent calls
 * would need a Map<offerId, port>.
 */
export const BridgeEndpointBinding = () => {
  if (!isElectron()) return null;

  return <BridgeEndpointBindingInner />;
};

const BridgeEndpointBindingInner = () => {
  const { data: sessions } = useP2PSessions();
  const sessionsRef = useRef(sessions);
  useEffect(() => {
    sessionsRef.current = sessions;
  }, [sessions]);

  useEffect(() => {
    if (!isElectron()) return;

    // offerIdMap: window offerId → chat messageId
    const offerIdMap: OfferIdMap = new Map();
    // Outgoing candidates that arrived before the offer's chat messageId was
    // known — held so they carry the correct offerMessageId once it resolves.
    const pendingCandidates = new Map<string, WindowToMainMessage[]>();
    let activePort: MessagePort | null = null;
    const unsubscribers: VoidFunction[] = [];

    // The call window closing is the reliable "call over" signal (covers local
    // end, peer end + terminal dwell, and manual close) — drop the return-to-call bar.
    unsubscribers.push(window.App.onCallEnded(() => setCallIdle()));

    // A MessagePort cannot cross the contextBridge — the preload transfers it
    // into the main world via window.postMessage; we receive it here.
    const onWindowMessage = (event: MessageEvent<{ __callBridgePort?: { launch: CallWindowLaunch } }>) => {
      // Only accept the port from our own preload (posts with source === window).
      // Embedded product webviews can postMessage the parent — reject those.
      // No origin comparison: packaged builds load `file://`, where `location.origin`
      // is "file://" but the delivered `event.origin` is the opaque "null".
      if (event.source !== window) return;
      const bridge = event.data?.__callBridgePort;
      if (!bridge) return;
      const port = event.ports[0];
      if (!port) return;
      const data = bridge;
      activePort = port;
      port.start();
      // A call is now live in the separate window — show the return-to-call bar
      // in the peer's chat view.
      setCallActive(data.launch.peerName, data.launch.peerSessionId);
      console.info('[CALL] bridge port received (direction=%s, offerId=%s)', data.launch.direction, data.launch.offerId);

      // Correlate this call to its peer's chat session. Falling back to the
      // first session (legacy behaviour) is only a safety net for a launch that
      // predates peerSessionId — it misroutes with 2+ contacts, so warn loudly.
      const peerSessionId = data.launch.peerSessionId;
      const resolveSession = () => {
        const list = sessionsRef.current;
        if (peerSessionId) {
          const match = list.find(s => s.sessionId === peerSessionId);
          if (match) return match;
          console.warn('[CALL] no session matches peerSessionId=%s — falling back to sessions[0]', peerSessionId);
        }
        return list[0] ?? null;
      };

      // For incoming calls: pre-seed the map so offerId == chatMessageId (no translation needed).
      // For outgoing: the offerId→messageId mapping is filled when publishOffer resolves.
      if (data.launch.direction === 'incoming') {
        // offerId == chatMessageId for incoming (set in IncomingCallDetector)
        offerIdMap.set(data.launch.offerId, data.launch.offerId);

        // The offer SDP was stashed by IncomingCallDetector when it opened the window.
        const sdp = takeIncomingOffer(data.launch.offerId) ?? new Uint8Array();
        console.info(
          '[CALL] provideIncomingCall → window (sdp=%d bytes%s)',
          sdp.length,
          sdp.length === 0 ? ' — STASH MISS!' : '',
        );

        const provideMsg: MainToWindowMessage = {
          kind: 'provideIncomingCall',
          offerId: data.launch.offerId,
          purpose: data.launch.purpose,
          sdp,
          candidates: new Uint8Array(),
          peerName: data.launch.peerName,
        };
        port.postMessage(provideMsg);
      }

      // A resolution for the active offer that is NOT in this set came from a sibling.
      const locallyHandledOffers = new Set<string>();

      const sendSignal = (msg: WindowToMainMessage): void => {
        if (msg.kind === 'publishAnswer' || msg.kind === 'publishClosed') {
          locallyHandledOffers.add(msg.offerId);
        }
        const session = resolveSession();
        const content = callService.translateWindowToMain(msg, offerIdMap);
        if (!content) return;
        if (!session) {
          console.warn('[CALL] no active session — cannot relay %s', msg.kind);
          return;
        }
        // Candidates are high-frequency — keep them at debug; answer/closed at info.
        const log = msg.kind === 'publishCandidates' ? console.debug : console.info;
        log('[CALL] relaying %s → chat (session=%s)', msg.kind, session.sessionId);
        session.sendMessage(content).catch((e: unknown) => console.error('[CALL] FAILED to relay %s:', msg.kind, e));
      };

      port.onmessage = (event: MessageEvent<unknown>) => {
        const msg = parseWindowToMain(event.data);
        if (!msg) return;
        const session = resolveSession();

        if (msg.kind === 'publishOffer') {
          if (!session) {
            console.warn('[CALL] publishOffer: no active session — cannot send offer');
            return;
          }
          const content = callService.translateWindowToMain(msg, offerIdMap);
          if (!content) return;
          console.info('[CALL] relaying publishOffer → chat (session=%s)', session.sessionId);
          session
            .sendMessage(content)
            .then(({ messageId }) => {
              offerIdMap.set(msg.offerId, messageId);
              console.info('[CALL] publishOffer SENT on-chain (messageId=%s ← offerId=%s)', messageId, msg.offerId);
              // Flush any candidates that raced ahead of the offer's messageId so
              // they now carry the correct offerMessageId the peer can correlate.
              const queued = pendingCandidates.get(msg.offerId);
              if (queued) {
                pendingCandidates.delete(msg.offerId);
                console.info('[CALL] flushing %d buffered candidate batch(es)', queued.length);
                for (const c of queued) sendSignal(c);
              }
            })
            .catch((e: unknown) => console.error('[CALL] FAILED to send offer:', e));
          return;
        }

        // Outgoing candidates before the offer is acked would carry the wrong
        // offerMessageId (the window UUID, not the chat messageId) — buffer them.
        if (msg.kind === 'publishCandidates' && !offerIdMap.has(msg.offerId)) {
          const queue = pendingCandidates.get(msg.offerId) ?? [];
          queue.push(msg);
          pendingCandidates.set(msg.offerId, queue);
          console.debug('[CALL] buffering candidates until offer acked (offerId=%s)', msg.offerId);
          return;
        }

        sendSignal(msg);
      };

      // Subscribe to the PEER's session message stream for inbound call signals.
      // Call signals are non-user "sync-carrier" messages, so we read the durable
      // stream: pairwise() skips the backlog present at subscription time, and the
      // direction filter drops our own outgoing signals so they don't echo back and
      // corrupt the peer connection. Subscribing only to the correlated session (not
      // every session) prevents another contact's call signals from being misrouted
      // into this window.
      const inboundSession = resolveSession();
      if (!inboundSession) {
        console.warn('[CALL] no session to watch for inbound call signals (peerSessionId=%s)', peerSessionId);
      } else {
        // The peer re-sends the answer many times; deliver it to the window once.
        const deliveredAnswerFor = new Set<string>();
        const sub = inboundSession.messages
          .pipe(
            pairwise(),
            concatMap(([previous, next]) => {
              const known = new Set(previous.map(m => m.messageId));

              return next.filter(m => !known.has(m.messageId) && m.status.direction === 'incoming');
            }),
          )
          .subscribe(m => {
            if (m.content.type !== 'callSignal') return;
            if (!activePort) return;
            const windowMsg = callService.translateInboundCallSignal(m, offerIdMap);
            if (!windowMsg) return;
            if (windowMsg.kind === 'deliverAnswer') {
              if (deliveredAnswerFor.has(windowMsg.offerId)) return; // drop duplicate answers
              deliveredAnswerFor.add(windowMsg.offerId);
            }
            // Candidates are noisy — debug; answer/closed at info.
            const log = m.content.signal === 'ice' ? console.debug : console.info;
            log('[CALL] delivering %s → call window (offerId=%s)', windowMsg.kind, windowMsg.offerId);
            activePort.postMessage(windowMsg);
          });
        unsubscribers.push(() => sub.unsubscribe());

        // Still ringing here, but resolved without this device publishing it: picked up
        // on a sibling. Close silently — a `closed` would tear down the sibling's call.
        // Needs the whole snapshot (the resolving message may already be in the backlog),
        // so it reads the raw stream rather than the new-messages pipe above.
        if (data.launch.direction === 'incoming') {
          let dismissed = false;
          const dismissSub = inboundSession.messages.subscribe(messages => {
            if (dismissed || !activePort) return;
            if (locallyHandledOffers.has(data.launch.offerId)) return;
            if (!callService.isIncomingOfferResolved(messages, data.launch.offerId)) return;
            dismissed = true;
            console.info(
              '[CALL] offer answered on a sibling device — dismissing ringing window (offerId=%s)',
              data.launch.offerId,
            );
            const dismissMsg: MainToWindowMessage = {
              kind: 'dismissAnsweredElsewhere',
              offerId: data.launch.offerId,
            };
            activePort.postMessage(dismissMsg);
          });
          unsubscribers.push(() => dismissSub.unsubscribe());
        }
      }
    };

    window.addEventListener('message', onWindowMessage);

    return () => {
      window.removeEventListener('message', onWindowMessage);
      for (const unsub of unsubscribers) {
        unsub();
      }
      activePort = null;
    };
    // Intentionally runs once: sets up the window-message listener + port relay
    // for the lifetime of the binding. sessionsRef keeps the session list current.
  }, []);

  return null;
};
