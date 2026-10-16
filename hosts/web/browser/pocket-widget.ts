/**
 * A product Widget for the browser test of Pocket: it reports the card it was
 * opened for, the product account and storage it sees, and whether the core
 * runs it as a Widget. Only a Widget may ask for the card face; this host
 * draws no face above an opened card, so a Widget is told `Unsupported`
 * where an App is `Denied`. What it saw is kept on `window.__widget`.
 */
import { getClientSync } from "@parity/truapi/sandbox";

interface Seen {
  card: string | null;
  account: string | null;
  stored: string | null;
  faceShown: string;
  errors: string[];
}

const seen: Seen = {
  card: new URLSearchParams(location.search).get("card"),
  account: null,
  stored: null,
  faceShown: "pending",
  errors: [],
};

const found = getClientSync();
if (found === null) throw new Error("no TrUAPI host");
const client = found;

const account = await client.account.getAccount({
  productAccountId: {
    dotNsIdentifier: "peopl.paseo",
    derivationIndex: { tag: "Index", value: 0 },
  },
});
if (account.isOk()) seen.account = account.value.account.publicKey;
else seen.errors.push(`account: ${JSON.stringify(account.error)}`);

const stored = await client.localStorage.read({ key: "fixture.from" });
if (stored.isOk()) seen.stored = stored.value.value ?? null;
else seen.errors.push(`storage: ${JSON.stringify(stored.error)}`);

const face = await client.expandedCard.setFaceShown({ shown: false });
seen.faceShown = face.isOk() ? "Applied" : face.error.tag;

(window as unknown as { __widget: Seen }).__widget = seen;
