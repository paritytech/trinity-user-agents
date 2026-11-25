import { ChatMessage as ChatMessageCodec } from '@novasamatech/host-chat/codec/message';
import { type CodecType } from 'scale-ts';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { chatContentService } from './service';

type ChatMessageInput = CodecType<typeof ChatMessageCodec>;

const BLURHASH = 'LKO2?U%2Tw=w]~RBVZRi};RPxuwH';

describe('chatContentService — file meta blurhash', () => {
  const imageWire = (thumbnail?: Uint8Array | null) => ({
    tag: 'image',
    value: { general: { mimeType: 'image/png', fileSize: 1000 }, width: 100, height: 80, thumbnail },
  });
  const videoWire = (thumbnail?: Uint8Array | null) => ({
    tag: 'video',
    value: { general: { mimeType: 'video/mp4', fileSize: 4096 }, duration: 30, thumbnail },
  });

  it('decodes the image thumbnail bytes to a blurhash string', () => {
    const out = chatContentService.mapFileMeta(imageWire(new TextEncoder().encode(BLURHASH)));
    expect(out).toMatchObject({ type: 'image', blurhash: BLURHASH });
  });

  it('decodes the video thumbnail bytes to a blurhash string', () => {
    const out = chatContentService.mapFileMeta(videoWire(new TextEncoder().encode(BLURHASH)));
    expect(out).toMatchObject({ type: 'video', blurhash: BLURHASH });
  });

  it.each([
    ['absent', undefined],
    ['null', null],
    ['empty', new Uint8Array()],
  ])('leaves blurhash unset when the thumbnail is %s', (_label, thumbnail) => {
    const out = chatContentService.mapFileMeta(imageWire(thumbnail));
    expect(out?.type === 'image' ? out.blurhash : 'unexpected-type').toBeUndefined();
  });

  it('rejects an oversized thumbnail instead of decoding it', () => {
    const out = chatContentService.mapFileMeta(imageWire(new Uint8Array(257).fill(0x61)));
    expect(out?.type === 'image' ? out.blurhash : 'unexpected-type').toBeUndefined();
  });

  type WireRichText = { attachments: { value: { meta: { value: { thumbnail?: Uint8Array } } } }[] };
  const firstThumbnail = (out: { value: unknown } | null): Uint8Array | undefined =>
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test reaches into the opaque {tag,value} wire shape
    (out!.value as WireRichText).attachments[0]!.value.meta.value.thumbnail;

  it('round-trips the blurhash back onto the wire thumbnail', () => {
    const out = chatContentService.mapUiContentToSdk({
      type: 'richText',
      attachments: [
        {
          identifier: new Uint8Array([1]),
          claimTicket: new Uint8Array([2]),
          meta: { type: 'image', mimeType: 'image/png', fileSize: 1000, width: 100, height: 80, blurhash: BLURHASH },
        },
      ],
    });

    const thumbnail = firstThumbnail(out);
    expect(thumbnail).toBeInstanceOf(Uint8Array);
    expect(new TextDecoder().decode(thumbnail)).toBe(BLURHASH);
  });

  it('writes no thumbnail when the attachment has no blurhash', () => {
    const out = chatContentService.mapUiContentToSdk({
      type: 'richText',
      attachments: [
        {
          identifier: new Uint8Array([1]),
          claimTicket: new Uint8Array([2]),
          meta: { type: 'video', mimeType: 'video/mp4', fileSize: 4096, duration: 30 },
        },
      ],
    });

    expect(firstThumbnail(out)).toBeUndefined();
  });
});

describe('contentMappers — coinage / send / call signals', () => {
  beforeEach(() => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
  });

  describe('mapSdkContent', () => {
    it('decodes coinagePayment as transfer (coinage)', () => {
      const out = chatContentService.mapSdkContent({
        tag: 'coinagePayment',
        value: { totalValue: 1_500_000_000_000n, coinKeys: [] },
      });
      expect(out).toEqual({ type: 'transfer', kind: 'coinage', amount: 1_500_000_000_000n });
    });

    it('decodes send (native) — assetId is null', () => {
      const blockHash = new Uint8Array(32).fill(0xaa);
      const extrinsicHash = new Uint8Array(32).fill(0xbb);
      const out = chatContentService.mapSdkContent({
        tag: 'send',
        value: { amount: 250n, assetId: null, blockHash, extrinsicHash },
      });
      expect(out).toEqual({
        type: 'transfer',
        kind: 'legacy',
        amount: 250n,
        assetId: null,
        blockHash,
        extrinsicHash,
      });
    });

    it('decodes send (asset) — preserves hex assetId', () => {
      const out = chatContentService.mapSdkContent({
        tag: 'send',
        value: { amount: 1n, assetId: '0xdeadbeef' },
      });
      expect(out).toMatchObject({ type: 'transfer', kind: 'legacy', amount: 1n, assetId: '0xdeadbeef' });
    });

    it('decodes dataChannelOffer (audio) with bare-string purpose — preserves sdp bytes', () => {
      const sdp = new Uint8Array([0x01, 0x02]);
      expect(chatContentService.mapSdkContent({ tag: 'dataChannelOffer', value: { sdp, purpose: 'AUDIO_CALL' } })).toEqual({
        type: 'callSignal',
        signal: 'offer',
        purpose: 'audio',
        sdp,
      });
    });

    it('decodes dataChannelOffer (video) with { tag, value } purpose — preserves sdp bytes', () => {
      const sdp = new Uint8Array([0x03, 0x04]);
      expect(
        chatContentService.mapSdkContent({
          tag: 'dataChannelOffer',
          value: { sdp, purpose: { tag: 'VIDEO_CALL', value: undefined } },
        }),
      ).toEqual({ type: 'callSignal', signal: 'offer', purpose: 'video', sdp });
    });

    it('decodes dataChannelAnswer / ice / closed and preserves offerMessageId + sdp bytes', () => {
      const sdp = new Uint8Array([0x05, 0x06]);
      const value = { offerMessageId: 'offer-42', sdp };
      expect(chatContentService.mapSdkContent({ tag: 'dataChannelAnswer', value })).toEqual({
        type: 'callSignal',
        signal: 'answer',
        offerMessageId: 'offer-42',
        sdp,
      });
      expect(chatContentService.mapSdkContent({ tag: 'dataChannelIceCandidate', value })).toEqual({
        type: 'callSignal',
        signal: 'ice',
        offerMessageId: 'offer-42',
        sdp,
      });
      // closed carries no sdp
      expect(chatContentService.mapSdkContent({ tag: 'dataChannelClosed', value: { offerMessageId: 'offer-42' } })).toEqual({
        type: 'callSignal',
        signal: 'closed',
        offerMessageId: 'offer-42',
      });
    });

    it('omits sdp field when the wire value carries no sdp bytes', () => {
      const out = chatContentService.mapSdkContent({ tag: 'dataChannelOffer', value: { purpose: 'AUDIO_CALL' } });
      expect(out).toEqual({ type: 'callSignal', signal: 'offer', purpose: 'audio' });
      expect(out && 'sdp' in out).toBe(false);
    });

    it('rejects a coinage payment payload missing totalValue', () => {
      expect(chatContentService.mapSdkContent({ tag: 'coinagePayment', value: { coinKeys: [] } })).toBeNull();
    });

    it('rejects an offer with unknown purpose', () => {
      expect(
        chatContentService.mapSdkContent({ tag: 'dataChannelOffer', value: { sdp: new Uint8Array(), purpose: 'MYSTERY' } }),
      ).toBeNull();
    });

    it('still warns + returns null on an unrecognised tag', () => {
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      expect(chatContentService.mapSdkContent({ tag: 'mysteryTag', value: undefined })).toBeNull();
      expect(warn).toHaveBeenCalledWith('[chat-content-mappers] Unknown SDK content tag:', 'mysteryTag');
    });
  });

  describe('mapUiContentToSdk', () => {
    it('returns null for transfer (desktop never sends)', () => {
      expect(chatContentService.mapUiContentToSdk({ type: 'transfer', kind: 'coinage', amount: 1n })).toBeNull();
    });

    it('encodes offer (video) → dataChannelOffer with VIDEO_CALL purpose and sdp', () => {
      const sdp = new Uint8Array([0x11, 0x22]);
      expect(chatContentService.mapUiContentToSdk({ type: 'callSignal', signal: 'offer', purpose: 'video', sdp })).toEqual({
        tag: 'dataChannelOffer',
        value: { sdp, purpose: 'VIDEO_CALL' },
      });
    });

    it('encodes offer (audio) → dataChannelOffer with AUDIO_CALL purpose and sdp', () => {
      const sdp = new Uint8Array([0x33, 0x44]);
      expect(chatContentService.mapUiContentToSdk({ type: 'callSignal', signal: 'offer', purpose: 'audio', sdp })).toEqual({
        tag: 'dataChannelOffer',
        value: { sdp, purpose: 'AUDIO_CALL' },
      });
    });

    it('encodes offer with no sdp → falls back to empty Uint8Array', () => {
      const out = chatContentService.mapUiContentToSdk({ type: 'callSignal', signal: 'offer', purpose: 'audio' });
      expect(out).toEqual({ tag: 'dataChannelOffer', value: { sdp: new Uint8Array(), purpose: 'AUDIO_CALL' } });
    });

    it('encodes answer → dataChannelAnswer with offerMessageId and sdp', () => {
      const sdp = new Uint8Array([0x55, 0x66]);
      expect(
        chatContentService.mapUiContentToSdk({ type: 'callSignal', signal: 'answer', offerMessageId: 'mid-1', sdp }),
      ).toEqual({ tag: 'dataChannelAnswer', value: { offerMessageId: 'mid-1', sdp } });
    });

    it('encodes ice → dataChannelIceCandidate with offerMessageId and sdp', () => {
      const sdp = new Uint8Array([0x77, 0x88]);
      expect(chatContentService.mapUiContentToSdk({ type: 'callSignal', signal: 'ice', offerMessageId: 'mid-2', sdp })).toEqual({
        tag: 'dataChannelIceCandidate',
        value: { offerMessageId: 'mid-2', sdp },
      });
    });

    it('encodes closed → dataChannelClosed with offerMessageId (no sdp)', () => {
      expect(chatContentService.mapUiContentToSdk({ type: 'callSignal', signal: 'closed', offerMessageId: 'mid-3' })).toEqual({
        tag: 'dataChannelClosed',
        value: { offerMessageId: 'mid-3' },
      });
    });

    it('encodes closed with no offerMessageId → falls back to empty string', () => {
      expect(chatContentService.mapUiContentToSdk({ type: 'callSignal', signal: 'closed' })).toEqual({
        tag: 'dataChannelClosed',
        value: { offerMessageId: '' },
      });
    });
  });
});

// ── Identity-channel events (absorbed from the former events.ts) ─────────

const encodeChatMessage = (timestamp: number, content: ChatMessageInput['versioned']['value']): Uint8Array =>
  ChatMessageCodec.enc({
    messageId: 'msg-1',
    timestamp: BigInt(timestamp),
    versioned: { tag: 'v1', value: content },
  });

describe('decodeEventsFromChatMessage iOS VoIP token', () => {
  afterEach(() => vi.restoreAllMocks());

  it('decodes an iOSVoIP push token (platform index 2) without a decode failure', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const bytes = encodeChatMessage(1_700_000_000_000, {
      tag: 'token',
      value: { token: '0xcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd', platform: 'iOSVoIP' },
    });

    const events = chatContentService.decodeEventsFromChatMessage(bytes);

    // A token carries no identity-channel event, but it must not throw/be dropped.
    expect(events).toEqual([]);
    expect(warn).not.toHaveBeenCalledWith(expect.stringContaining('ChatMessage decode failed'), expect.anything());
  });
});

describe('decodeEventsFromChatMessage acceptedAt', () => {
  it('carries the wire timestamp on a deviceChatAccepted accept signal', () => {
    const acceptedAt = 1_700_000_000_000;
    const bytes = encodeChatMessage(acceptedAt, {
      tag: 'deviceChatAccepted',
      value: {
        requestId: 'req-1',
        device: {
          statementAccountId: new Uint8Array(32).fill(0xaa),
          encryptionPublicKey: new Uint8Array(32).fill(0xbb),
        },
      },
    });

    const events = chatContentService.decodeEventsFromChatMessage(bytes);

    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({ tag: 'acceptSignal', signal: { requestId: 'req-1', acceptedAt } });
  });

  it('drops Android-legacy chatAccepted @14 signals (no DeviceInfo on the wire)', () => {
    // Accepting chatAccepted @14 would force the matcher into the
    // identity-conflated synthetic-device fallback, which the peer cannot
    // decrypt (bug #9, blocked-on-Android). Decoder must drop these silently
    // until Android emits deviceChatAccepted @20.
    const acceptedAt = 1_700_000_999_000;
    const bytes = encodeChatMessage(acceptedAt, { tag: 'chatAccepted', value: { messageId: 'req-2' } });

    const events = chatContentService.decodeEventsFromChatMessage(bytes);

    expect(events).toHaveLength(0);
  });
});
