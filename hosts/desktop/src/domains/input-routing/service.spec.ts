import { describe, expect, it } from 'vitest';

import { CANDIDATE_CAP, CONTEXT_CAP } from './constants';
import { inputRoutingService } from './service';
import { type AttachmentCategory, type InputResponse, type QueryRecipient } from './types';

const ALL_CATEGORIES: AttachmentCategory[] = ['audio', 'video', 'image', 'file'];

const recipient = (productId: string, text: boolean, attachment: AttachmentCategory[] = []): QueryRecipient => ({
  productId,
  declaration: { text, attachment },
});

const candidates = (...texts: string[]): InputResponse => ({
  type: 'candidates',
  candidates: texts.map(text => ({ type: 'text', text })),
});

describe('attachmentCategoryOf', () => {
  it.each([
    ['image/png', 'image'],
    ['audio/mpeg', 'audio'],
    ['video/mp4', 'video'],
    ['application/pdf', 'file'],
    ['text/plain; charset=utf-8', 'file'],
  ])('derives %s as %s', (mimeType, expected) => {
    expect(inputRoutingService.attachmentCategoryOf(mimeType)).toBe(expected);
  });

  it('labels a type-less attachment application/octet-stream, which is a file — not a guess', () => {
    expect(inputRoutingService.normalizeMimeType('')).toBe('application/octet-stream');
    expect(inputRoutingService.attachmentCategoryOf('')).toBe('file');
  });

  it('is case insensitive', () => {
    expect(inputRoutingService.attachmentCategoryOf('IMAGE/PNG')).toBe('image');
  });
});

describe('textRecipients', () => {
  it('takes only workers that declared text', () => {
    const set = inputRoutingService.textRecipients([recipient('a', true), recipient('b', false), recipient('c', true)]);

    expect(set.map(entry => entry.productId)).toEqual(['a', 'c']);
  });

  it('returns nothing when no worker declares text', () => {
    expect(inputRoutingService.textRecipients([recipient('a', false)])).toEqual([]);
  });

  it('truncates to the context cap', () => {
    const many = Array.from({ length: CONTEXT_CAP + 10 }, (_, index) => recipient(`p${index}`, true));

    expect(inputRoutingService.textRecipients(many)).toHaveLength(CONTEXT_CAP);
  });
});

describe('attachmentRecipients', () => {
  it('offers a product only the categories it declared', () => {
    const recipients = [
      recipient('images', false, ['image']),
      recipient('docs', false, ['file']),
      recipient('everything', false, ALL_CATEGORIES),
      recipient('text-only', true, []),
    ];

    expect(inputRoutingService.attachmentRecipients(recipients, 'image').map(entry => entry.productId)).toEqual([
      'images',
      'everything',
    ]);
    expect(inputRoutingService.attachmentRecipients(recipients, 'file').map(entry => entry.productId)).toEqual([
      'docs',
      'everything',
    ]);
  });

  it('is empty when nothing matches — the picker says so rather than falling back', () => {
    expect(inputRoutingService.attachmentRecipients([recipient('images', false, ['image'])], 'file')).toEqual([]);
  });

  it('ignores an unrecognized declared category', () => {
    // RFC-0027 § Registration: an unrecognized member is ignored, and the rest of
    // the declaration still applies.
    const odd: QueryRecipient[] = [
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- a manifest may declare a category this host does not know
      recipient('odd', true, ['hologram', 'image'] as AttachmentCategory[]),
    ];

    expect(inputRoutingService.attachmentRecipients(odd, 'image')).toHaveLength(1);
    expect(inputRoutingService.attachmentRecipients(odd, 'audio')).toHaveLength(0);
  });

  it('is not truncated by the context cap — a picker is not a broadcast', () => {
    const many = Array.from({ length: CONTEXT_CAP + 10 }, (_, index) => recipient(`p${index}`, false, ALL_CATEGORIES));

    expect(inputRoutingService.attachmentRecipients(many, 'image')).toHaveLength(CONTEXT_CAP + 10);
  });
});

describe('exceedsPayloadCap', () => {
  it('accepts an ordinary response', () => {
    expect(inputRoutingService.exceedsPayloadCap(candidates('a short answer'))).toBe(false);
  });

  it('rejects an oversized one whole', () => {
    expect(inputRoutingService.exceedsPayloadCap(candidates('x'.repeat(70_000)))).toBe(true);
  });

  it('counts custom payload bytes', () => {
    const response: InputResponse = {
      type: 'candidates',
      candidates: [{ type: 'custom', candidateId: 'c1', contentType: 'x', payload: new Uint8Array(70_000) }],
    };

    expect(inputRoutingService.exceedsPayloadCap(response)).toBe(true);
  });
});

describe('rankCandidates', () => {
  it('interleaves across products, preserving each product order', () => {
    const ranked = inputRoutingService.rankCandidates([
      { productId: 'a', response: candidates('a1', 'a2') },
      { productId: 'b', response: candidates('b1') },
    ]);

    expect(ranked.map(entry => entry.productId)).toEqual(['a', 'b', 'a']);
    expect(ranked.map(entry => (entry.content.type === 'text' ? entry.content.text : ''))).toEqual(['a1', 'b1', 'a2']);
  });

  it('drops notHandled responses', () => {
    const ranked = inputRoutingService.rankCandidates([
      { productId: 'a', response: { type: 'notHandled' } },
      { productId: 'b', response: candidates('b1') },
    ]);

    expect(ranked.map(entry => entry.productId)).toEqual(['b']);
  });

  it('caps each response so one product cannot occupy the list', () => {
    const flooder = candidates(...Array.from({ length: CANDIDATE_CAP + 20 }, (_, index) => `x${index}`));
    const ranked = inputRoutingService.rankCandidates([{ productId: 'flooder', response: flooder }]);

    expect(ranked).toHaveLength(CANDIDATE_CAP);
  });

  it('gives a flooding product one leading slot, not the whole head of the list', () => {
    const flooder = candidates(...Array.from({ length: CANDIDATE_CAP }, (_, index) => `x${index}`));
    const ranked = inputRoutingService.rankCandidates([
      { productId: 'flooder', response: flooder },
      { productId: 'honest', response: candidates('the good answer') },
    ]);

    expect(ranked.slice(0, 2).map(entry => entry.productId)).toEqual(['flooder', 'honest']);
  });

  it('is empty when every worker declines', () => {
    const ranked = inputRoutingService.rankCandidates([
      { productId: 'a', response: { type: 'notHandled' } },
      { productId: 'b', response: { type: 'notHandled' } },
    ]);

    expect(ranked).toEqual([]);
  });
});
