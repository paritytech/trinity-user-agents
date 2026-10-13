# Glossary

The domain vocabulary of the iOS app. Use these terms in issues, commits and code review so that a name means one thing.

## Terms

**Shelf.** The upward growth of the tab bar from its last item, the Apps item. It holds a grid of open products, with a settings and a connection status pair at its top.

**Center panel.** The panel the tab bar's center button morphs into. It currently hosts the scanner.

**Fold.** The visibility state of the tab bar. The bar is in one of three states:

- *Shown*: the bar sits at the bottom of the screen.
- *Folded*: the bar slides to the screen edge and leaves a grab area that brings it back.
- *Hidden*: the bar is gone entirely, and tab navigation is not allowed on that screen.

**Pocket.** The name of the wallet tab. The tab is titled Pocket; `wallet` is its code name (`TabBarItem.wallet`).

**Pocket (product cards).** The product card collection in code, separate from the tab. `ProductPocketService` holds the store the cards are read from, and `pocketCards` on a product worker lists the cards it offers.

## Removed vocabulary

Terms that appear in this repository's history but name nothing in the app today.
They are recorded here so nobody reintroduces one assuming it once meant something
else, and so a search that turns up an old commit or comment has somewhere to land.

### Terms

**DIM.** An opaque internal label. It is never expanded anywhere in the codebase
and stands for nothing. It named a family of two features, DIM1 and DIM2, together
with the layers built over them.

**DIM1.** A tattoo Proof-of-Ink evidence flow. A user documented a tattoo on video
and submitted it as evidence toward a personhood claim.

**DIM2.** A weekly video gesture game. Players joined a scheduled call and performed
gestures together. It was the only mechanism that proved personhood.

**Mob Rule.** The dispute and voting layer over Proof-of-Ink evidence, the way a
tattoo claim could be challenged and judged by other users.

**Prizes.** A branding skin applied to the DIM2 bot identity in chat.

**Personhood.** Proof that a user is a unique human, which raised that user's
transaction allowance above the device uniqueness base allowance. DIM2 was its only
proving mechanism, and the app has no other.

**Proof-of-Ink.** The evidence model behind a DIM1 tattoo claim.

**Collectibles.** A viewer for the NFT rewards the game paid out, reachable from the
wallet main screen. With no game there are no rewards and no game account to hold
them.

**Full username registration.** A username claim signed through the personhood
origin factory. The app registers usernames through the lite claim
(`ClaimLiteUsernameWireframe`).

### Notes

This code is not compiled in the Release configuration. No shipped build has contained
these features, so no production install holds data from them.

`BuiltInProduct` declares `dim2(for:)` and `personhood(for:)` as static functions on a
caseless enum, not as cases. The enum is a record of governance reserved names rather
than ordinary code, and removing a Swift symbol does not unclaim a name on chain. Any
feature that returns has to reuse the exact strings (`dim2.<tld>`, `peopl.<tld>`) to
recover the on-chain identity.

The personhood *product handlers* are live and unrelated to the removed pipeline.
`APPersonhoodHandlerFactory` and the handlers it builds serve ring-VRF operations for
products over sign-in, and Coinage reads the key domains the personhood product
identity supplies.
