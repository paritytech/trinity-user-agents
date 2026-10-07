# wasm-worker-probe

Runs a wasm32 module as a Worker product against a TrUAPI host, so a Worker
can be checked without JavaScript. The host is unchanged: it sees SCALE
frames on its product-frame endpoint, the same as from a JS worker.

Two crates:

- `wasm-worker-probe` (this crate): the runner. It connects to the endpoint
  a `truapi-host` process prints, instantiates the module under wasmi with a
  fuel budget per entry point and a linear-memory cap, calls `on_start`,
  relays each binary frame to `on_frame` and each frame the guest emits back
  to the socket, and calls `on_suspend` and `on_resume` on a timer. Every
  frame, decoded with the host's envelope codec and named from the generated
  wire table, and every line the guest logs go to stdout and `--transcript`
  as one JSON object per line.
- `wasm-worker-probe-guest`: the counter bot. On start it sends
  `system.handshake`, `chat.registerBot`, `chat.createRoom` and starts
  `renderer.actionSubscribe`; once the room exists it posts one `Custom`
  message. When the host opens `renderer.render` for that message it streams
  a tree with the count and one `bump` button. A `bump` action increments the
  count and redraws every open stream; suspend and resume redraw with a
  `paused` marker. The guest never blocks on the host.

  Two cargo features change what the wasm32 build draws. `pocket` makes no
  Chat call and draws only a Pocket card. `unified` starts as the default
  does and also draws a Pocket card, from the same count, so one product
  shows that count in its chat message and on its card; a `bump` in either
  redraws both. The two features exclude each other.

The guest's sandbox boundary is two imports, `host.frame_send` and
`host.log`, and the exports `alloc`, `free`, `on_start`, `on_frame`,
`on_suspend`, `on_resume`. Its wire discriminants are hardcoded, because the
protocol-only `truapi` build has no wire table, and pinned to the generated
table by this crate's tests.

## Running it against the CLI host

The CLI host has no surface that opens `renderer.render`, so the render leg
is driven by a probe: with `TRUAPI_RENDER_PROBE=<path>` set, the first
`Custom` chat message a product posts opens a render stream for it, every
tree is appended to `<path>`, and after the first tree one `bump` action is
published into the product's renderer action stream.

```bash
# Build the guest for wasm32 and the two binaries.
cargo build -p wasm-worker-probe-guest --target wasm32-unknown-unknown --release
cargo build -p truapi-host-cli -p wasm-worker-probe

# A headless signing host serving a Worker execution on a TCP endpoint. The
# first run of a new session registers a username on chain, which takes
# minutes; a session name that is already taken is refused.
TRUAPI_CHAT_LOG=/tmp/chat-host-messages.jsonl \
TRUAPI_RENDER_PROBE=/tmp/render-probe.jsonl \
target/debug/truapi-host signing-host --serve --auto-accept \
  --execution-kind worker --frame-listen 127.0.0.1:9731 \
  --product-id counter.dot --base-path /tmp/truapi-wasm-probe --session <name> &

# Once it prints "Signing host ready": the guest, with a suspend at 4 s,
# a resume at 5 s, and a clean close at 8 s.
target/debug/wasm-worker-probe \
  --module target/wasm32-unknown-unknown/release/wasm_worker_probe_guest.wasm \
  --frame-url ws://127.0.0.1:9731 \
  --transcript /tmp/runner-transcript.jsonl \
  --suspend-after 4 --run-for 8
```

What the three transcripts then show:

- `runner-transcript.jsonl`: the four opening frames out, the handshake, bot
  and room responses in, `chat_post_message` out and its response in, the
  host-initiated `renderer_render` start in with four `receive` trees out
  (`count 0`, `count 1`, `count 1 · paused`, `count 1`), the
  `renderer_action_subscribe` item in between the first two, and the fuel
  each turn burned.
- `chat-host-messages.jsonl` (`CliChatHost`): the bot, the room, and message
  `m1` with the `Custom` payload bytes as the host received them.
- `render-probe.jsonl`: `render-opened` for `m1`, each tree as the host
  decoded it, `action-published`, and `render-closed` when the product
  disconnected.

Bounded execution: `--fuel 1000` is below what `on_start` needs, so the
sandbox traps, the runner reports `ran out of fuel` and exits 1 without the
host seeing a frame.

## What this is not

It is not a worker `runtime` kind in the manifest, not a Store that installs
a `.wasm` worker, and not a native app running one. The vendored mobile hosts
run JS workers in WebViews and are untouched. Nothing here changes the
protocol, the core, or any product.
