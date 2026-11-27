---
title: "Product safe area"
owner: "Sergey Kononenko"
status: draft
---

# RFC — Product safe area

## Summary

Every product page covers the whole screen. A host overlay is a view the host draws over an edge of the page, such as
the status strip, a navigation bar, the tab bar, or a Pocket card face. The host reports where its overlays sit through
the WebView's safe area, so the page reads it as `env(safe-area-inset-*)` and places its own content. The host adds no
script and does not paint the page. The same rule holds in a tab, on a pushed screen, and in an opened Pocket card, of
which [Expanded card layering](expanded-card-layering.md) is one case.

## Motivation

A host draws overlays at the top of the screen, such as a status strip and a navigation bar, and a floating tab bar at
the bottom that the user can tuck away. Today a product page cannot tell where the overlays are. Either the host insets
the page itself, so the page cannot draw under the overlays and keeps a gap when the tab bar tucks away, or the host
lays the page under the overlays and they cover the page's top row and bottom controls.

## Requirements

- **Coverage:** a page can draw under every host overlay, edge to edge.
- **Placement:** a page knows where the host overlays are through standard CSS, with no host-specific API.
- **Portability:** one page works in every host presentation, on every host, and in a plain browser.
- **Isolation:** the host injects no script into the page to place it or to style itself.
- **Stability:** the page's viewport does not resize when a host overlay moves.

## Approach

The design has four parts:

- What the page covers.
- What the safe area contains.
- How the page opts in.
- Who owns the colours under the host overlays.

### Coverage

The host lays the product's WebView over the whole screen in every presentation and adds no padding or content inset of
its own, so the page's viewport is the screen. A host overlay that moves, such as the tab bar tucking away or a
navigation bar hiding, does not resize the viewport.

### Safe area

The host reports each edge's inset as the distance from that edge of the viewport to the inner edge of the host overlays
there. It includes system areas, the parts of the screen the operating system reserves, such as its status bar, a
display cutout, or its navigation gesture area.

Each inset counts the following areas:

| Edge   | Areas counted                                                                 |
| ------ | ----------------------------------------------------------------------------- |
| Top    | System areas, host status strip, navigation bar while shown, Pocket card face |
| Bottom | System areas, tab bar while shown at rest                                     |
| Sides  | System areas only                                                             |

A host builds the insets on its platform's own safe area mechanism, which the WebView already maps to
`env(safe-area-inset-*)`. When a host overlay changes state, such as the tab bar tucking or showing, the host updates
the inset once the overlay settles, and the engine lays the page out again with no script and no reload. The keyboard is
not part of the bottom inset; a page follows it through the visual viewport as in any browser.

### Opt-in

A page receives safe areas only with `viewport-fit=cover` in its viewport, as in any browser. Any viewport tag the host
adds keeps the page's `viewport-fit`. A page that does not opt in reads every inset as 0 and draws under the host
overlays.

### Colours

The page owns everything it paints, including the area under the host overlays. Host overlays take the host theme, which
the page already receives through the [theme subscription](extended-theme-api.md), so a page that follows the host theme
stays readable under them. The host does not read the page's colours or restyle its overlays for a page.

## Trade-offs

- **Overlays cannot adapt to a page.** A page that paints against the host theme under the host overlays makes them hard
  to read, and the host does nothing about it.
- **The page animates inset changes.** The host applies a new inset in one step. Animating the content that follows an
  inset is up to the page.
- **Pages must opt in.** A page without `viewport-fit=cover` loses its top row and bottom controls under the host
  overlays.
