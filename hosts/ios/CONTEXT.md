# Removed vocabulary

Terms that appear in this repository's history but name nothing in the app today.
They are recorded here so nobody reintroduces one assuming it once meant something
else, and so a search that turns up an old commit or comment has somewhere to land.

## Terms

**DIM.** An opaque internal label. It is never expanded anywhere in the codebase
and stands for nothing. It named a family of two features, DIM1 and DIM2, together
with the layers built over them.

**DIM1.** A tattoo Proof-of-Ink evidence flow. A user documented a tattoo on video
and submitted it as evidence toward a personhood claim.

**DIM2.** A weekly video gesture game. Players joined a scheduled call and performed
gestures together. It was the only mechanism that ever proved personhood.

**Mob Rule.** The dispute and voting layer over Proof-of-Ink evidence, the way a
tattoo claim could be challenged and judged by other users.

**Prizes.** A branding skin applied to the DIM2 bot identity in chat.

**Personhood.** Proof that a user is a unique human, which raised that user's
transaction allowance above the device uniqueness base allowance. DIM2 was its only
proving mechanism, so it did not outlive the game.

**Proof-of-Ink.** The evidence model behind a DIM1 tattoo claim.

**Collectibles.** A viewer for the NFT rewards the game paid out, reachable from the
wallet main screen. With no game there are no rewards and no game account to hold
them.

**Full username registration.** A username claim signed through the personhood
origin factory. The lite claim is now the only way to register a username. It is not
a replacement, it always existed alongside the full claim and depends on nothing that
was removed.

## Notes

The Release configuration never compiled any of this. No shipped build has ever
contained these features, so no production user was affected by their removal and
no production install holds data from them.

`BuiltInProduct` still carries its `dim2` and `personhood` cases. The enum is a
record of governance reserved names rather than ordinary code, and deleting a Swift
constant does not unclaim a name on chain. Anything re-added later has to reuse the
exact strings to recover the on-chain identity.

The personhood *product handlers* are live and unrelated to the removed pipeline.
`APPersonhoodHandlerFactory` and the handlers it builds serve ring-VRF operations for
products over sign-in, and Coinage reads the key domains the personhood product
identity supplies.
