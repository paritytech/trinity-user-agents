# Chat materials demo worker

A worker that opens one chat room and answers every message there with a card whose text sits inside
a `Materials` node. On a host that draws materials the card shows the layers of the material over
`Daily Dollar` and `Tilt your phone`. While the material loads, or when it cannot draw, the card
shows those two lines plain.

## Material

`MATERIAL_ID_PLACEHOLDER` in `src/worker.ts` names no published material. Set it to the Bulletin ID
of a material published on the network the app runs on before trying the card on a device.

## Build

No published `@parity/truapi` carries the `Materials` node yet, so `package.json` takes the client
from the enclosing `trinity-user-agents` tree. Build it there first:

```sh
( cd ../../../../../.. && ./scripts/codegen.sh )
( cd ../../../../../../js/packages/truapi && npm install && npm run build )
npm install
npm run build        # dist/worker.js
```

## Publish and try it

Publish `dist/worker.js` under the `worker.<name>.<tld>` dotNS name of the product and set its
`executable` text record to `manifest/worker.json`. The load loop is the one in
[`hosts/android/docs/pocket-card-dev-loop.md`](../../../../docs/pocket-card-dev-loop.md). Open the
`Materials demo` room in the app and send any message.
