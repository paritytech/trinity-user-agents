// @vitest-environment happy-dom
//
// `service.ts` imports `productService` from the product barrel, which pulls the
// recents repository and its module-scope `persistLocalStorage` along with it.

import { describe, expect, it } from 'vitest';

import { type RankedCandidate } from '@/domains/input-routing';

import { inputModalityService } from './service';

const base = { screenProductId: null, dashboardProductIds: ['wallet', 'notes'], roomProductIds: ['chatty'] };

describe('contextProductIds', () => {
  it('contributes the one product on a product route', () => {
    const ids = inputModalityService.contextProductIds({ ...base, pathname: '/product/wallet', screenProductId: 'wallet' });

    expect(ids).toEqual(['wallet']);
  });

  it('contributes every product with a room on the chat route', () => {
    expect(inputModalityService.contextProductIds({ ...base, pathname: '/chat' })).toEqual(['chatty']);
    expect(inputModalityService.contextProductIds({ ...base, pathname: '/chat/room-1' })).toEqual(['chatty']);
  });

  it('contributes every placed product on the dashboard', () => {
    expect(inputModalityService.contextProductIds({ ...base, pathname: '/dashboard' })).toEqual(['wallet', 'notes']);
  });

  it('contributes nothing on favorites, settings, or an unknown route', () => {
    expect(inputModalityService.contextProductIds({ ...base, pathname: '/favorites' })).toEqual([]);
    expect(inputModalityService.contextProductIds({ ...base, pathname: '/settings' })).toEqual([]);
    expect(inputModalityService.contextProductIds({ ...base, pathname: '/whatever' })).toEqual([]);
  });

  it('contributes nothing on a product route with no product resolved from the screen', () => {
    expect(inputModalityService.contextProductIds({ ...base, pathname: '/product/wallet' })).toEqual([]);
  });

  it('does not treat a lookalike route as a match', () => {
    // `/chatbot` is not `/chat`; a prefix test would leak the chat context set.
    expect(inputModalityService.contextProductIds({ ...base, pathname: '/chatbot' })).toEqual([]);
  });
});

describe('installedAmong', () => {
  it('drops a favourited native id, which is not a product', () => {
    expect(inputModalityService.installedAmong(['chat', 'wallet'], ['wallet', 'notes'])).toEqual(['wallet']);
  });

  it('is empty when nothing placed is installed', () => {
    expect(inputModalityService.installedAmong(['chat'], ['wallet'])).toEqual([]);
  });
});

describe('distinctProductIds', () => {
  it('collapses many rooms with one product into one id', () => {
    const rooms = [{ productId: 'wallet' }, { productId: 'wallet' }, { productId: 'notes' }];

    expect(inputModalityService.distinctProductIds(rooms)).toEqual(['wallet', 'notes']);
  });
});

describe('suggestProducts', () => {
  const product = (baseName: string, displayName: string) =>
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- only the two fields the derivation reads
    ({ baseName, displayName }) as unknown as Parameters<typeof inputModalityService.suggestProducts>[1][number];

  const all = [product('wallet.dot', 'Wallet'), product('browse.dot', 'Browse'), product('notes.dot', 'Notes')];

  it('shows everything when nothing is typed', () => {
    const { recentProducts, installed } = inputModalityService.suggestProducts('', all, ['browse.dot']);

    expect(recentProducts.map(p => p.baseName)).toEqual(['browse.dot']);
    expect(installed.map(p => p.baseName)).toEqual(['wallet.dot', 'notes.dot']);
  });

  it('narrows both sections as the user types', () => {
    const { recentProducts, installed } = inputModalityService.suggestProducts('brow', all, ['browse.dot']);

    expect(recentProducts.map(p => p.baseName)).toEqual(['browse.dot']);
    expect(installed).toEqual([]);
  });

  it('empties out for a real query, leaving the surface to the candidates', () => {
    // The whole point: "send 5 DOT to alice" names no product, so no host row
    // should sit above the answers that do address it.
    const { recentProducts, installed, allItems } = inputModalityService.suggestProducts('send 5 DOT to alice', all, [
      'browse.dot',
    ]);

    expect(recentProducts).toEqual([]);
    expect(installed).toEqual([]);
    expect(allItems).toEqual([]);
  });

  it('never lists the same product twice, so arrow keys always move to a new one', () => {
    const { allItems } = inputModalityService.suggestProducts('', all, ['browse.dot', 'wallet.dot']);

    expect(allItems.map(p => p.baseName)).toEqual(['browse.dot', 'wallet.dot', 'notes.dot']);
  });

  it('drops a recent id that is no longer installed', () => {
    const { recentProducts } = inputModalityService.suggestProducts('', all, ['uninstalled.dot', 'notes.dot']);

    expect(recentProducts.map(p => p.baseName)).toEqual(['notes.dot']);
  });
});

describe.each(['.dot', '.paseo'])('ghostSuffix on %s', tld => {
  const ghost = (input: string) => inputModalityService.ghostSuffix(input, tld);

  it('returns empty for empty / whitespace input', () => {
    expect(ghost('')).toBe('');
    expect(ghost('   ')).toBe('');
  });

  it('appends the network TLD to bare names', () => {
    expect(ghost('foo')).toBe(tld);
    expect(ghost('  foo  ')).toBe(tld);
  });

  it('does not append for inputs already ending in the TLD', () => {
    expect(ghost(`foo${tld}`)).toBe('');
    expect(ghost(`foo${tld}.li`)).toBe('');
  });

  it('does not append when the TLD appears before a path / query / hash', () => {
    expect(ghost(`foo${tld}/bar`)).toBe('');
    expect(ghost(`foo${tld}?x=1`)).toBe('');
    expect(ghost(`foo${tld}#frag`)).toBe('');
    expect(ghost(`foo${tld}.li/bar`)).toBe('');
  });

  it('does not append to a bare name carrying a path / query / hash', () => {
    expect(ghost('foo/bar')).toBe('');
    expect(ghost('foo?x=1')).toBe('');
    expect(ghost('foo#frag')).toBe('');
  });

  it('does not append for http(s) URLs', () => {
    expect(ghost('http://example.com')).toBe('');
    expect(ghost('https://example.com')).toBe('');
    expect(ghost('HTTPS://EXAMPLE.COM')).toBe('');
  });

  it('does not append for localhost / 127.0.0.1', () => {
    expect(ghost('localhost')).toBe('');
    expect(ghost('localhost:3000')).toBe('');
    expect(ghost('127.0.0.1')).toBe('');
    expect(ghost('127.0.0.1:8080/foo')).toBe('');
  });

  it('appends for inputs that contain the TLD but not as a suffix', () => {
    expect(ghost(`foo${tld.slice(1)}li`)).toBe(tld);
  });

  // The leading dot must stay escaped when the suffix is interpolated into the
  // guard: unescaped, `.` matches any character and `xpaseo` would read as
  // already suffixed.
  it('appends when the label is present without its leading dot', () => {
    expect(ghost(`x${tld.slice(1)}`)).toBe(tld);
  });
});

describe('groupByProduct', () => {
  const candidate = (productId: string, id: string): RankedCandidate => ({
    id,
    productId,
    content: { type: 'text', text: id },
  });

  it('orders groups by where each product first placed', () => {
    // What ranking produces: round-robin, so the products interleave.
    const groups = inputModalityService.groupByProduct([
      candidate('wallet', 'wallet:0'),
      candidate('notes', 'notes:0'),
      candidate('wallet', 'wallet:1'),
    ]);

    expect(groups.map(group => group.productId)).toEqual(['wallet', 'notes']);
    // The top-ranked candidate stays first in the first group, so Enter still
    // takes the same answer it took before grouping.
    expect(groups[0]?.candidates[0]?.id).toBe('wallet:0');
  });

  it('keeps each product’s answers in its own rank order', () => {
    const groups = inputModalityService.groupByProduct([
      candidate('wallet', 'wallet:0'),
      candidate('notes', 'notes:0'),
      candidate('wallet', 'wallet:1'),
      candidate('wallet', 'wallet:2'),
    ]);

    expect(groups[0]?.candidates.map(entry => entry.id)).toEqual(['wallet:0', 'wallet:1', 'wallet:2']);
    expect(groups[1]?.candidates.map(entry => entry.id)).toEqual(['notes:0']);
  });

  it('has no groups when nothing answered', () => {
    expect(inputModalityService.groupByProduct([])).toEqual([]);
  });
});
