# Working on a Pocket card

Two loops. The first proves a face decodes and draws with no worker, no manifest
and no product in the way; the second proves the whole card, live. Build in that
order. A card that does not draw is almost always a face problem, and the first
loop tells you that in seconds.

Everything below is Debug-only and reached from **Settings → Debug**.

## Loop A: the face preview

**Debug → Pocket Face Preview.** Type a URL, press Draw, and the face is fetched,
decoded and drawn in the real card frame. A face the decoder refuses shows the
decoder's own message, which is the one that says what is wrong with the file.

Serve faces from anywhere on your machine; the simulator reaches `127.0.0.1`
directly, so nothing needs forwarding:

```sh
cd <your faces directory> && python3 -m http.server 5173 --bind 127.0.0.1
```

Then draw `http://127.0.0.1:5173/pocket/<name>.json`.

The size bound is the same one the approval sheet applies (256 KiB), so a face
that draws here is one the sheet will accept.

## Loop B: the whole card, live

A product with **no published worker**, driven by hand.

1. **Debug → Products → Add.** Give it a dotNS name and the URL of your built
   worker bundle. A published worker always wins, so use a name that publishes
   none, or the hand-installed script is never reached.
2. **Debug → Pocket Cards.** Add a card against that same name: card id, title
   and the URL of its static face. The card id is screened here with the rules
   the core applies, so a bad one is refused with a message rather than reading
   as no card later.
3. Open the add deeplink. The screen shows it, ready to copy:

   ```
   polkadot://<product>/-/pocket/add?card=<id>
   ```

   In a Debug build the scheme is the brand scheme plus the flavor suffix, so on
   the simulator:

   ```sh
   xcrun simctl openurl booted 'polkadotappdev://<product>/-/pocket/add?card=<id>'
   ```

4. Approve it. The card joins the Pocket on the Wallet tab, drawn with the face
   the sheet showed.
5. Scroll the card into view. That is what makes it live: a visible card holds
   one worker reference, which boots the worker, and releases it when the card
   leaves the screen. Presses and text edits go straight back to the worker from
   the collection. Pressing the card opens its product at
   `https://<product>?card=<id>`, on a screen the card heads: card and product
   share one scroll, so scrolling past the card leaves the product the whole
   screen.

## What fails where

Work down this list. Each step rules out the one below it.

| Symptom | Where to look |
|---|---|
| Face does not draw in Loop A | the face file, or the decoder |
| Card draws static but never updates | the worker is not running or not reachable |
| Card never appears at all | the card id, or `includes.pocket` on the worker |
| Presses do nothing | the worker is not running, or registered no handler for that action |
| A pinned card will not go away | it is privileged, and no removal will move it |

## Traps

- **The worker bundle must be a single ASCII file.** Worker JS is read as
  Latin-1. Minify it: bundlers keep identifiers ASCII but copy comments through
  verbatim, and some dependencies' own doc comments carry em dashes.
- **Serve with `Cache-Control: no-store` and force-quit between runs.** A worker
  is kept warm and its bundle cached, so rebuilding alone leaves you reading
  results from the code you replaced.
- **A card id that fails screening reads as no card**, not as an error. If a card
  never arrives, check the id before anything else.
- **Confirm a tap actually lands** before concluding an action was dropped.
