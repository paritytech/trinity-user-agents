# Developer tools

The window has two zones: the app, and the developer tools beside or under it. The **Focus capsule** (Dotli's 120 × 8
capsule, top centre) shows or hides the tools. Its colour is the core's state: grey starting, green signed in, amber
signed out, red failed.

## Tabs

- **Account:** core state, network and [chain connections](networks.md#chain-backends), wallet sign-in and import, test
  accounts, Account status.
- **Product:** open a product (name or URL, recents, product id), the open product (Close, Reset its data, What it may
  use, Details) and Workers.
- **Preview:** the app's size and frame.
- **Inspector:** six views. See [Inspector](inspector.md).
- **Chat** (dev server, or a build with `WEB_SIGNING_KETTLE_CHAT=1`): which chat products run in the tab, which one New
  chat starts in, and Kettle Chat's log recovery over iroh (off by default; on, the public test relays or a custom
  relay). See [Dev server controls](contact-chat.md#dev-server-controls).
- **More:** Versions, and dev-server controls: the invitation demo, request delivery and the reply key (the request
  service's, for every product that uses it), and sample Inbox items.

## Search field

The Pill opens one field, **Find a person or app** (`src/app/search-card.ts`). Typing opens nothing and picks no one.

- **Apps:** matching recents. Enter, the go button or **Open** opens the typed address.
- **People** (`src/contacts/search-people.ts`): only for a signed-in wallet and text that can be a username. Contacts
  first, then the picker's username directory. Blocked people never show. A person found by username is checked on chain
  before a choice.
- Choosing a person opens Chat as New chat does (`ChatScreen.startWith`). Nothing goes to a product. A choice from
  before a session change is dropped.
- The close button, `Escape`, a tap on the card's handle or a drag down from the handle or the heading closes the card.
- `Escape` closes the suggestions, then the card. A failed address reopens the card with the reason.
- Scanning a code is not available.

## Product id

- The Product tab's **Product id** sets the id that the core uses. Empty means derived: `localhost:<port>`, the name, or
  the host name. A bare IP derives an id that the core refuses, so enter one.
- The id applies on Open, not on edit. **Reset** clears an override.
- The id selects the accounts, grants, storage and privileges that the page gets, including signing without a prompt. It
  is not a check that the page owns the id. Enter only ids of products that you develop.

## Layout

- **Over 900 px:** the tools sit on the right (480 px) or at the bottom. Drag or arrow-key (24 px) the inner edge: width
  320 px to all but 360 px, height 160 px to all but 120 px. The browser keeps each size
  (`truapi-web-signing-host:devtools-width`, `...:devtools-height`).
- **Narrow screens:** the tools cover the app with a scrim (a 380 px panel, or a bottom sheet on a phone). They start
  hidden. `Escape`, the scrim or the close button closes them. On a phone, a tap on the sheet's handle or a drag down
  from the handle or the header also closes it; the sheet keeps its size.
- Showing, hiding, moving or resizing the tools never re-creates the product frame. The product does not reload.

## Preview

- Sizes: **Auto** (Desktop from 760 px, else Phone), **Desktop**, **Phone** (393 × 852), and under **All sizes**,
  Tablet, Desktop 1280 × 800 and Custom.
- The app lays out at exactly that CSS size and shrinks to fit.
- **Frame:** **Device** (bezel, island, home indicator), **Round** or **No frame**. Host screens, the Docklet and the
  product keep clear of the island.
- A line shows the product frame's size, which is smaller than the screen.
- A new tab starts on Phone with the device frame. On a narrow screen the app fills the screen.

## Versions

- Client, host, core, provider and debugger versions, from the linked packages and the loaded `testing` bundle.
- A warning shows only when the core's wire schema differs from the client's.
- **Technical details:** SHA-256 of the served `.wasm` files, the wire schema, and the source commit with a dirty flag.
- The values are measured when the dev server starts or the build runs. Restart the dev server after you rebuild a WASM
  bundle.

## Reload

A tab reopens its product after a reload, once its wallet is restored. A `?product=` link only fills the form.
