// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';

import { dotNsService } from '@/domains/product';

import { decideDidNavigate, decideDidNavigateInPage, decideWillNavigate } from './navigation';

const TLD = '.dot';

function parsed(url: string) {
  const r = dotNsService.parseDotNsDomain(url, TLD);
  if (!r) throw new Error(`fixture: cannot parse ${url}`);
  return r;
}

describe('no settled TLD', () => {
  // A localhost product names no dotNS registry, so it is still decided — the
  // no-auth suites never resolve a TLD at all and their whole surface is
  // localhost. Only the dotNS judgement waits.
  it('still intercepts a localhost navigation', () => {
    expect(
      decideWillNavigate({ tld: null, url: 'http://localhost:5173/page', identifier: 'localhost:5173', localhost: true }),
    ).toEqual({ type: 'sync-pathname', pathname: 'page', track: false });
    expect(
      decideDidNavigateInPage({
        tld: null,
        url: 'http://localhost:5173/new',
        identifier: 'localhost:5173',
        localhost: true,
        isMainFrame: true,
      }),
    ).toEqual({ type: 'sync-pathname', pathname: 'new', track: true });
  });

  // The whole point of the null: guessing `.dot` here would intercept a link
  // belonging to no namespace this network serves. Only that judgement defers.
  it('lets a dotNS navigation through rather than judging it against a guessed namespace', () => {
    expect(decideWillNavigate({ tld: null, url: 'polkadot://other.dot/x', identifier: 'app.dot', localhost: false })).toEqual({
      type: 'allow',
    });
    expect(decideDidNavigate({ tld: null, url: 'polkadot://other.dot/x', identifier: 'app.dot' })).toEqual({ type: 'allow' });
    expect(
      decideDidNavigateInPage({
        tld: null,
        url: 'polkadot://other.dot/x',
        identifier: 'app.dot',
        localhost: false,
        isMainFrame: true,
      }),
    ).toEqual({ type: 'allow' });
  });

  it('still denies a dangerous scheme', () => {
    expect(decideWillNavigate({ tld: null, url: 'javascript:alert(1)', identifier: 'app.dot', localhost: false })).toEqual({
      type: 'deny',
    });
  });
});

describe('decideWillNavigate', () => {
  it('allows non-dotNs URLs', () => {
    expect(decideWillNavigate({ tld: TLD, url: 'https://example.com/x', identifier: 'app.dot', localhost: false })).toEqual({
      type: 'allow',
    });
  });

  it('syncs and tracks pathname for same-product polkadot://', () => {
    expect(decideWillNavigate({ tld: TLD, url: 'polkadot://app.dot/page', identifier: 'app.dot', localhost: false })).toEqual({
      type: 'sync-pathname',
      pathname: 'page',
      track: true,
    });
  });

  it('cross-product polkadot:// → cross-product with stop:true', () => {
    expect(decideWillNavigate({ tld: TLD, url: 'polkadot://other.dot/x', identifier: 'app.dot', localhost: false })).toEqual({
      type: 'cross-product',
      target: parsed('polkadot://other.dot/x'),
      stop: true,
    });
  });

  it('legacy nested encoding (polkadot://this/polkadot://other/...) → cross-product, stop:false', () => {
    const url = 'polkadot://app.dot/polkadot://other.dot/x';
    expect(decideWillNavigate({ tld: TLD, url, identifier: 'app.dot', localhost: false })).toEqual({
      type: 'cross-product',
      target: parsed('polkadot://other.dot/x'),
      stop: false,
    });
  });

  it('same-product nested encoding still triggers cross-product (stop:false)', () => {
    const url = 'polkadot://app.dot/polkadot://app.dot/x';
    expect(decideWillNavigate({ tld: TLD, url, identifier: 'app.dot', localhost: false })).toEqual({
      type: 'cross-product',
      target: parsed('polkadot://app.dot/x'),
      stop: false,
    });
  });

  it('parsed but not dotDomain and not sameLocalhost → allow', () => {
    expect(decideWillNavigate({ tld: TLD, url: 'polkadot://other.NOT-dot/x', identifier: 'app.dot', localhost: false })).toEqual({
      type: 'allow',
    });
  });

  it('http://localhost/x in localhost mode → sync-pathname track:false', () => {
    expect(
      decideWillNavigate({ tld: TLD, url: 'http://localhost:5173/page', identifier: 'localhost:5173', localhost: true }),
    ).toEqual({
      type: 'sync-pathname',
      pathname: 'page',
      track: false,
    });
  });

  it('http://localhost/x with non-localhost identifier → allow', () => {
    expect(decideWillNavigate({ tld: TLD, url: 'http://localhost:5173/page', identifier: 'app.dot', localhost: false })).toEqual({
      type: 'allow',
    });
  });

  it('garbage / empty inputs → allow', () => {
    expect(decideWillNavigate({ tld: TLD, url: '', identifier: 'app.dot', localhost: false })).toEqual({ type: 'allow' });
    expect(decideWillNavigate({ tld: TLD, url: 'not-a-url', identifier: 'app.dot', localhost: false })).toEqual({
      type: 'allow',
    });
  });

  it.each([
    'javascript:alert(1)',
    'data:text/html,<script>alert(1)</script>',
    'blob:https://example.com/abc',
    'file:///etc/passwd',
  ])('rejects dangerous scheme %s', url => {
    expect(decideWillNavigate({ tld: TLD, url, identifier: 'app.dot', localhost: false })).toEqual({ type: 'deny' });
  });
});

describe('decideDidNavigate', () => {
  it('allows non-polkadot URLs', () => {
    expect(decideDidNavigate({ tld: TLD, url: 'https://example.com/x', identifier: 'app.dot' })).toEqual({ type: 'allow' });
  });

  it('allows same-product polkadot URL', () => {
    expect(decideDidNavigate({ tld: TLD, url: 'polkadot://app.dot/x', identifier: 'app.dot' })).toEqual({ type: 'allow' });
  });

  it('reverts when committed to a cross-product URL (race loss)', () => {
    expect(decideDidNavigate({ tld: TLD, url: 'polkadot://other.dot/x', identifier: 'app.dot' })).toEqual({
      type: 'revert-to-desired',
    });
  });

  it('reverts when committed to legacy-nested cross-product URL', () => {
    expect(decideDidNavigate({ tld: TLD, url: 'polkadot://app.dot/polkadot://other.dot/x', identifier: 'app.dot' })).toEqual({
      type: 'revert-to-desired',
    });
  });

  it('allows when polkadot URL fails to parse', () => {
    expect(decideDidNavigate({ tld: TLD, url: 'polkadot://', identifier: 'app.dot' })).toEqual({ type: 'allow' });
  });
});

describe('decideDidNavigateInPage', () => {
  it('syncs and tracks pathname on same-product main-frame SPA nav', () => {
    expect(
      decideDidNavigateInPage({
        tld: TLD,
        url: 'polkadot://app.dot/new',
        identifier: 'app.dot',
        localhost: false,
        isMainFrame: true,
      }),
    ).toEqual({ type: 'sync-pathname', pathname: 'new', track: true });
  });

  it('ignores cross-product SPA nav (no dispatch)', () => {
    expect(
      decideDidNavigateInPage({
        tld: TLD,
        url: 'polkadot://other.dot/new',
        identifier: 'app.dot',
        localhost: false,
        isMainFrame: true,
      }),
    ).toEqual({ type: 'allow' });
  });

  it('ignores subframe navigations', () => {
    expect(
      decideDidNavigateInPage({
        tld: TLD,
        url: 'polkadot://app.dot/new',
        identifier: 'app.dot',
        localhost: false,
        isMainFrame: false,
      }),
    ).toEqual({ type: 'allow' });
  });

  it('ignores non-dotNs URLs', () => {
    expect(
      decideDidNavigateInPage({
        tld: TLD,
        url: 'https://example.com/x',
        identifier: 'app.dot',
        localhost: false,
        isMainFrame: true,
      }),
    ).toEqual({ type: 'allow' });
  });

  it('handles localhost identifier same-product', () => {
    expect(
      decideDidNavigateInPage({
        tld: TLD,
        url: 'http://localhost:5173/new',
        identifier: 'localhost:5173',
        localhost: true,
        isMainFrame: true,
      }),
    ).toEqual({ type: 'sync-pathname', pathname: 'new', track: true });
  });
});

// The decision helpers must key off the network's TLD, not a baked-in `.dot`:
// under `.paseo` a `.paseo` name is the product and a `.dot` name is a stranger.
describe('on a network whose TLD is not .dot', () => {
  const PASEO = '.paseo';

  it('syncs and tracks pathname for a same-product polkadot:// URL', () => {
    expect(
      decideWillNavigate({ tld: PASEO, url: 'polkadot://app.paseo/page', identifier: 'app.paseo', localhost: false }),
    ).toEqual({ type: 'sync-pathname', pathname: 'page', track: true });
  });

  it('treats a cross-product polkadot:// URL as cross-product', () => {
    expect(
      decideWillNavigate({ tld: PASEO, url: 'polkadot://other.paseo/x', identifier: 'app.paseo', localhost: false }),
    ).toEqual({
      type: 'cross-product',
      target: { identifier: 'other.paseo', pathname: 'x' },
      stop: true,
    });
  });

  it('reverts a polkadot:// URL naming another product', () => {
    expect(decideDidNavigate({ tld: PASEO, url: 'polkadot://other.paseo/x', identifier: 'app.paseo' })).toEqual({
      type: 'revert-to-desired',
    });
  });

  it('syncs an in-page navigation within the same product', () => {
    expect(
      decideDidNavigateInPage({
        tld: PASEO,
        url: 'polkadot://app.paseo/deep',
        identifier: 'app.paseo',
        localhost: false,
        isMainFrame: true,
      }),
    ).toEqual({ type: 'sync-pathname', pathname: 'deep', track: true });
  });

  it('allows a .dot URL, which is not a product on this network', () => {
    expect(decideWillNavigate({ tld: PASEO, url: 'https://app.dot/x', identifier: 'app.paseo', localhost: false })).toEqual({
      type: 'allow',
    });
  });
});
