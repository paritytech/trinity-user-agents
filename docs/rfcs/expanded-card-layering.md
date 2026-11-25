---
title: "Expanded card layering"
owner: "@ERussel"
status: draft
---

# RFC — Expanded card layering

## Summary

An opened Pocket card shows the product page in a transparent WebView that covers the whole screen, above the native
face. The page scrolls the face with it, starts its content below the face, which the host makes part of the page's top
safe area, and may draw over the face. `ExpandedCard::set_face_shown` keeps its wire shape and comes to mean whether the
page covers the face. Everything else about Pocket and the expanded card stays as [Pocket modality](pocket-modality.md)
defines it.

## Motivation

[Pocket modality](pocket-modality.md#expanded-card) has the host draw the face above the Widget, move the face out of
the way on `set_face_shown(false)`, and size the Widget to the space the face leaves. On iOS the card screen then nests
the WebView's scroll view inside a scroll view that carries the face, and keeps the two in step by hand: it moves the
face with the page and resizes the page after each gesture. The two scroll views fight over gestures, and every resize
makes the page lay out again.

Products also need the face to stay on screen while the page draws modal content over it. For example, The Humanity
product's daily-dollar scratch card is a full-screen route that calls `expandedCard.setFaceShown({ shown: false })`
while it is open. Under pocket-modality that moves the face away instead of letting the scratch card sit over it.

## Requirements

- **Scrolling:** the page and the face scroll as one column in one scroll view.
- **Layering:** the page can draw over the face without the face leaving the screen.
- **Stability:** the page is never resized because the face moved or was covered.
- **Wire:** the `ExpandedCard` wire format does not change.
- **Fallback:** one page works under its card, in a tab, and on hosts that follow pocket-modality's layout.

## Approach

The design has five parts:

- The executable the card opens.
- How the face and the page are layered.
- The safe area that tells the page where the face ends.
- How the two scroll.
- What `set_face_shown` means.

### Executable

Tapping a face opens the product's `widget` executable, or its `app` executable when the product publishes no widget,
with `card=<card_id>` in the launch URL as pocket-modality defines. Either one runs as a Widget execution, so either can
call `ExpandedCard`.

### Layering

The host draws the face at the top of the screen and the page in a transparent WebView above it, covering the whole
screen. Wherever the page paints, it is drawn over the face; where it paints nothing, the face shows through. The page
lays out its content below the face and may draw over the face, for example a modal or a full-screen surface.

### Safe area

The host makes the face part of the WebView's safe area, so the page reads where the face ends as
`env(safe-area-inset-top)`, the value it already uses for the status bar and the notch. A host builds it from its
platform's own safe area: the view's safe area on iOS, the window insets it hands the WebView on Android. While the card
is open the top inset is the distance from the top of the viewport to the bottom of the face, with the document scrolled
to the top. Covering the face does not change it, and the engine lays the page out again if the face ever moves, with no
script and no reload.

A page receives safe areas only with `viewport-fit=cover` in its viewport, and the host keeps the page's `viewport-fit`
in any viewport it adds. In its normal state the page paints nothing above the top inset and starts its content there. A
surface that covers the face places itself without it, for example with `position: fixed; inset: 0`. Where no face is
drawn the top inset is the status bar or 0, so the same page works in a tab, in a browser, and on any host.

### Scrolling

The face follows the WebView's scroll offset as if it were the first thing in the document, overscroll past the top
included, so the page and the card read as one column. There is one scroll view, and the host never resizes the page for
the face.

### Face state

`set_face_shown(false)` marks the face covered: it stays drawn where it is, and touches over it go to the page.
`set_face_shown(true)` sends touches over the face back to the face. Neither call moves the face or resizes the page. A
card definition's `faceShown` sets whether the face starts covered, with `false` meaning covered. Covering applies to
the card whichever side is up, and turning the card keeps it covered or uncovered. Gestures that move the card, such as
pulling it down to close it, start only while the face is uncovered. A covered page is still never a trap: the host
keeps a way to leave the card, such as its back navigation or tab bar.

`UserMoving` means the user is moving the card, such as pulling it down to close it, and the request has no effect.
`NotPresented` keeps its meaning.

### Compatibility

A host that keeps pocket-modality's layout works with a page that respects the top safe area: the page sits below the
face, its top inset is the status bar or 0, and covering the face still gives the page the face's area because that host
moves the face away. A page that paints its whole viewport opaque hides the face on a layered host.

## Trade-offs

- **Touches route by state, not by pixels.** The host cannot see what the page painted, so a page that draws a control
  over the face without covering it loses those taps to the face.
- **A covered face is inert.** Face actions are out of reach while the face is covered.
- **The card shares the safe area with the system.** From `env()` alone a page cannot tell the face from the status bar,
  so a surface that covers the face and must clear only the status bar uses a margin of its own.
- **Pages need `viewport-fit=cover`.** Without it the engine reports a safe area of 0, and the page draws over the face.
