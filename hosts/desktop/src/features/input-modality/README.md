# input-modality

The spotlight overlay: one surface for typing an address, searching your products, and scanning a code. It opens
over whatever is on screen, never as a page of its own.

It is also the host side of **RFC-0027 (Input Routing)** — fanning a query out to the products the screen
underneath contributes, and drawing what they answer. **That half is currently parked.** It is written, compiled,
typechecked and covered by tests; it just never runs.

## What ships today

- Opens on Cmd/Ctrl+L, Cmd/Ctrl+K and Cmd/Ctrl+T, and on a click in the address bar — all four raise
  `focusAddressBarSideEffect`, which `feature.tsx` handles. The surface owns no shortcut of its own.
- Closes on Escape and on a click outside the card.
- Opens on what the address bar was showing, selected, when a product is on screen — the bar passes it as
  `initialText` on `focusAddressBarSideEffect`, and only the bar passes it: a new tab starts from nothing. Product
  routes only; a native subject's display id (`chat`) names a surface, not an address.
- Suggests products as you type: recents first, then saved, both narrowing on the query.
- Resolves a typed identifier that isn't installed into a **Go to** row, from its manifest, into memory — naming
  a product is not installing one.
- Tab completes `.dot`; Enter opens what the field names, or the suggestion you arrowed to.
- Scans a QR code. A code naming a product opens it; anything else lands in the field as typed text.

## What is parked

Every block is marked `PARKED` in the source. Nothing is deleted.

| parked                                                                                                   | where                                    |
| -------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| candidate cards, per-product grouping, "From this screen", "asked N products", "Nothing can handle this" | `ui/InputSpotlight.tsx`                  |
| the attachment path — paperclip, file input, chips, size-declined line, recipient picker                 | `ui/InputSpotlight.tsx`                  |
| "Nothing on this screen can answer that"                                                                 | `ui/InputSpotlight.tsx`                  |
| the demo answer handlers                                                                                 | `feature.tsx`                            |
| the four tests that asserted the above                                                                   | `ui/InputSpotlight.test.tsx`             |
| `title` / `placeholder` copy — was `Ask` / `Ask anything…`                                               | `src/shared/translation/locales/en.json` |

Untouched and still live in the tree, just unreached: `state/round.ts`, `state/composer.ts`, `service.ts`,
`hooks/useInputContext.ts`, `ui/CandidateCard.tsx`, `ui/CustomCandidate.tsx`, `ui/AttachmentChip.tsx`,
`ui/AttachmentPicker.tsx`, `ui/ProductIdentity.tsx`, `demoAnswers.ts`, `@/widgets/RendererTree` and all of
`@/domains/input-routing`. `service.spec.ts`, `state/round.spec.ts` and `ProductIdentity.test.tsx` still run, so
the parked logic cannot rot silently.

`npm run knip` reports the unreached files as unused. That is the expected reading of a parked feature, not
something to fix by deleting them.

## Why it is off

**The context set is empty, so no product is ever asked.** `state/round.ts` resolves a navigation _before_ it
looks at the recipients, and returns `IDLE` for a query whose context set is empty — "a screen with nothing on it
asks nobody, so no product learns the string". The surface passes `[]`, and that one argument is the whole
disable: deeplinks still resolve on their debounce, while a query produces no recipients, no gateway call, no
`input_request` when it is one day wired, and no demo answer.

This is the RFC's own mechanism rather than a flag on top of it — which is why the domain needed no change at
all.

## Turning querying back on

1. `rg 'PARKED' src/features/input-modality` — uncomment every block it finds, in `ui/InputSpotlight.tsx`,
   `feature.tsx` and `ui/InputSpotlight.test.tsx`. Each block names what it restores, including the imports that
   were trimmed from the top of each file.
2. In `ui/InputSpotlight.tsx`, restore the round effect to its real context set:

   ```ts
   const productIds = useInputContext();
   // …
   useEffect(() => {
     requestRound(entry.text, productIds, entry.source);
   }, [entry, productIds]);
   ```

3. In `SpotlightResults`, restore `hasBody` to `hasOtherContent || round.kind === 'query' || noContext` — the
   parked one-liner only knows about suggestions.
4. Restore the two copy keys under `feature.inputModality` in `src/shared/translation/locales/en.json`:
   `title` → `Ask`, `placeholder` → `Ask anything…`. No other locale carries them.
5. `npm run types && npm run lint && npx vitest run src/features/input-modality`.

The demo handlers in `feature.tsx` are a separate switch. Leaving them commented is the honest default — with no
handler registered the transformers answer nothing, every product declines, and that is what an unwired host
should show. Uncomment them only to see the surface with answers in it.

## Boundaries

- The domain is `@/domains/input-routing` — it owns routing, ranking and delivery, and never renders. Its
  `README.md` is the vocabulary and the divergences from RFC-0027; read it before changing anything here.
- The context set is derived from the screen by `hooks/useInputContext.ts` and passed **down** as a parameter.
  Deriving it is host-UI knowledge, which is why it lives in the feature and not in the domain.
- Opening a product is `product`'s job, reached through `openDotNsUrlSideEffect` from `@/features/browser`. This
  feature never navigates directly.
