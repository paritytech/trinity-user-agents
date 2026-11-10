import { type DotNsUrl, dotNsService } from '@/domains/product';

export type NavigationDecision =
  | { type: 'allow' }
  | { type: 'deny' }
  | { type: 'sync-pathname'; pathname: string; track: boolean }
  | { type: 'cross-product'; target: DotNsUrl; stop: boolean }
  | { type: 'revert-to-desired' };

// Renderer-side defense in depth. The main process also blocks these via
// isAllowedHostNavigation, but if anything ever broadens that allowlist, the
// webview tag still refuses these schemes here.
const DANGEROUS_SCHEMES = new Set(['javascript:', 'data:', 'blob:', 'file:']);

function hasDangerousScheme(url: string): boolean {
  try {
    return DANGEROUS_SCHEMES.has(new URL(url).protocol);
  } catch {
    return false;
  }
}

type WillNavigateArgs = { url: string; identifier: string; localhost: boolean; tld: Nullable<string> };

export function decideWillNavigate({ url, identifier, localhost, tld }: WillNavigateArgs): NavigationDecision {
  if (hasDangerousScheme(url)) return { type: 'deny' };

  const dotNsUrl = dotNsService.parseDotNsDomain(url, tld);
  if (!dotNsUrl) return { type: 'allow' };

  const isDotDomainUrl = dotNsService.isDotDomain(dotNsUrl.identifier, tld);
  const isSameLocalhost = localhost && dotNsUrl.identifier === identifier;
  if (!isDotDomainUrl && !isSameLocalhost) return { type: 'allow' };

  if (url.startsWith('polkadot://')) {
    const nestedLink = dotNsService.parseDotNsDomain(dotNsUrl.pathname, tld);
    if (nestedLink && dotNsService.isDotDomain(nestedLink.identifier, tld)) {
      return { type: 'cross-product', target: nestedLink, stop: false };
    }

    if (dotNsUrl.identifier !== identifier) {
      return { type: 'cross-product', target: dotNsUrl, stop: true };
    }

    return { type: 'sync-pathname', pathname: dotNsUrl.pathname, track: true };
  }

  return { type: 'sync-pathname', pathname: dotNsUrl.pathname, track: false };
}

type DidNavigateArgs = { url: string; identifier: string; tld: Nullable<string> };

export function decideDidNavigate({ url, identifier, tld }: DidNavigateArgs): NavigationDecision {
  if (!url.startsWith('polkadot://')) return { type: 'allow' };

  const dotNsUrl = dotNsService.parseDotNsDomain(url, tld);
  if (!dotNsUrl) return { type: 'allow' };

  if (dotNsUrl.identifier !== identifier) return { type: 'revert-to-desired' };

  const nestedLink = dotNsService.parseDotNsDomain(dotNsUrl.pathname, tld);
  if (nestedLink && dotNsService.isDotDomain(nestedLink.identifier, tld)) {
    return { type: 'revert-to-desired' };
  }

  return { type: 'allow' };
}

type DidNavigateInPageArgs = { url: string; identifier: string; localhost: boolean; isMainFrame: boolean; tld: Nullable<string> };

export function decideDidNavigateInPage({
  url,
  identifier,
  localhost,
  isMainFrame,
  tld,
}: DidNavigateInPageArgs): NavigationDecision {
  if (!isMainFrame) return { type: 'allow' };

  const dotNsUrl = dotNsService.parseDotNsDomain(url, tld);
  if (!dotNsUrl) return { type: 'allow' };

  const isDotDomainUrl = dotNsService.isDotDomain(dotNsUrl.identifier, tld);
  const isSameLocalhost = localhost && dotNsUrl.identifier === identifier;
  if (!isDotDomainUrl && !isSameLocalhost) return { type: 'allow' };
  if (dotNsUrl.identifier !== identifier) return { type: 'allow' };

  return { type: 'sync-pathname', pathname: dotNsUrl.pathname, track: true };
}
