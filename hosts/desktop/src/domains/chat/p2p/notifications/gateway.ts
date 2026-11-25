/**
 * Push notification sending for P2P chat.
 *
 * After a message is confirmed delivered to the statement store,
 * sends a fire-and-forget HTTP POST to the push notification backend
 * so the recipient's device gets a push notification.
 *
 * Mirrors Android's RealPushNotificationHelper / ChatPushNotificationsSender.
 */

import { type MessageContent, ChatMessage as ChatMessageCodec } from '@novasamatech/host-chat/codec/message';
import { type Encryption } from '@novasamatech/statement-store';
import { type CodecType } from 'scale-ts';

import { pushNotificationService } from './service';

type MessageContentType = CodecType<typeof MessageContent>;

export type SendPushNotificationParams = {
  deviceToken: string;
  peerPlatform: 'Android' | 'iOS' | undefined;
  sharedSecret: Uint8Array;
  encryption: Encryption;
  localAccountId: Uint8Array;
  remoteAccountId: Uint8Array;
  messageId: string;
  timestamp: number;
  content: MessageContentType;
  /** Identity-backend root; the caller resolves it, so this gateway stays environment-free. */
  backendUrl: string;
  /** APNs topic for iOS peers. Ignored for Android. */
  iosBundleId: string;
};

/**
 * Send a push notification to the peer's device. Fire-and-forget.
 *
 * 0. Drop contents the peer cannot present as a notification
 * 1. SCALE-encode the ChatMessage
 * 2. Encrypt with the session's ChaCha20-Poly1305 encryption
 * 3. POST to the push backend
 */
async function sendPushNotification(params: SendPushNotificationParams): Promise<void> {
  const {
    deviceToken,
    peerPlatform,
    sharedSecret,
    encryption,
    localAccountId,
    remoteAccountId,
    messageId,
    timestamp,
    content,
    backendUrl,
    iosBundleId,
  } = params;

  // The gate sits here rather than at the call site: every send path is a peer of this one, and a
  // non-notifiable content reaching the backend surfaces on the peer as an "Unsupported message"
  // banner (iOS `NewMessageHandler` has no silent branch), not as a dropped push.
  if (!pushNotificationService.isNotifiableContent(content.tag)) return;

  try {
    // 1. Compute pushId
    const pushId = pushNotificationService.computePushId(sharedSecret, localAccountId, remoteAccountId);

    // 2. SCALE-encode the ChatMessage
    const encoded = ChatMessageCodec.enc({
      messageId,
      timestamp: BigInt(timestamp),
      versioned: { tag: 'v1' as const, value: content },
    });

    // 3. Encrypt
    const encryptResult = encryption.encrypt(encoded);
    if (encryptResult.isErr()) {
      console.warn('[push-notification] Encryption failed:', encryptResult.error);

      return;
    }

    // 4. Build request body — convert device token to platform-appropriate format
    const platformToken = pushNotificationService.getPlatformDeviceToken(deviceToken, peerPlatform);
    const body: Record<string, unknown> = {
      deviceToken: platformToken,
      pushId: pushNotificationService.bytesToHexString(pushId),
      message: pushNotificationService.bytesToHexString(encryptResult.value),
      voip: pushNotificationService.isVoIPContent(content.tag),
    };

    // iOS needs platform + bundlerId (APNs topic) for the backend to route via APNs.
    // Without these the backend defaults to FCM (Android) and fails.
    // Mirrors iOS APNSClientService's NotifyRequestParameters.
    if (peerPlatform === 'iOS') {
      body['platform'] = 'ios';
      body['bundlerId'] = iosBundleId;
    } else if (peerPlatform === 'Android') {
      body['platform'] = 'android';
    }

    // 5. POST (fire-and-forget) — use Electron main process to bypass CORS
    const url = `${backendUrl}/api/v1/notify`;
    const bodyStr = JSON.stringify(body);

    let status: number;
    if (typeof window !== 'undefined' && window.App?.proxyFetch) {
      const res = await window.App.proxyFetch({
        url,
        method: 'POST',
        headers: [['Content-Type', 'application/json']],
        body: new TextEncoder().encode(bodyStr),
      });
      status = res.status;
    } else {
      const fetchRes = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: bodyStr,
      });
      status = fetchRes.status;
    }

    if (status < 200 || status >= 300) {
      console.warn('[push-notification] Backend returned', status, 'for message', messageId);
    }
  } catch (e) {
    console.warn('[push-notification] Failed to send:', e);
  }
}

export const pushNotificationGateway = {
  sendPushNotification,
};
