---
"@parity/truapi": minor
---

`game.remindNextGame` and `game.cancelNextGame` serve Humanity (`peopl.<tld>`) as well as the game product (`dim2.<tld>`), so Humanity's worker can book the reminder once it runs the weekly game in the Polkadot room. Each product holds and cancels only its own reminder. Every other product, including a subname such as `app.peopl.dot`, still gets `Unsupported`.
