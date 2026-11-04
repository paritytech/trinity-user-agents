# Wallets and sessions

## Wallets

- Import a wallet from its BIP-39 recovery phrase. The host does not create, register or attest accounts. The
  account-creation project does that.
- One phrase is one wallet, however often you import it.
- The picker skips a malformed saved entry and leaves it in storage. The log counts them.
- If `truapi-web-signing-host:wallets:v1` is not a list, the host refuses to save or forget. Fix it in DevTools.

## Test accounts

- The picker lists **Alice**, **Bob** and **Carol (test account)** under **Shared test accounts**, in every build. They
  come from public BIP-39 test vectors (`src/platform/dev-accounts.ts`), each with its own scope (`test-account:<id>`).
- No account is chosen until you pick one and choose **Sign in**. They run as an imported wallet does, with no
  **Forget**.
- **The phrases are public.** Anyone can sign as them, read their chat requests and use their daily slots. Keep nothing
  of value on them.
- The host registers and funds nothing for them. On Paseo Next v2 they are registered with chat keys as `testalice.10`,
  `testbob.10` and `testcarol.10`.
- The host refuses the Substrate development phrase (`src/invitations/gate.ts`) for chat and contacts. It is not one of
  these accounts.

## Account status

Read-only. It reads when you open the fold or press Refresh, and never polls.

- **Account:** the keys, and `Resources.Consumers` and `PeopleLite.LitePeople` for the identity account and the root
  key, at the finalized People block. It shows the username and Lite or Person credibility.
  - "Not found" means only that no record exists. "Unreadable" means that the runtime changed a layout that the product
    SDK's metadata describes.
  - **Ring** rows show where `Members.Members` holds the ring key that the account's records name
    (`PeopleLite.LitePeople`, or `People.AccountToPersonalId` and `People.People`): Onboarding, Included with its index,
    or Suspended. "No ring key on record" does not mean that the person is outside the rings. The rows are a diagnostic,
    not a signing explanation.
- **Current product:**
  - **Allocation recorded** is the core's local renewal ledger: history, not a chain check, not remaining quota.
  - **Account 0** is the product account at `DerivationIndex::Index(0)`, from `account.getAccount` through a provider
    that the runtime makes for the product. The core returns only the public key. The call opens no prompt (read from
    the source, not observed).
  - **Asset Hub balance** reads `System.Account` of account 0 at the finalized block. "No account entry" is not zero.
    Decimals come from `chainSpec_v1_properties`. It is the native balance only.
  - Not shown: Statement Store, Bulletin and PGAS. Their `//allowance//…` accounts are not product accounts, and PGAS
    needs metadata that this page cannot decode without a new dependency.

## Username sync

- The core gives a local session only the lite username that it is activated with. So the host supplies it.
- After sign-in, and on Refresh, the host reads `Resources.Consumers` for the identity account, then the root key.
- When the lite username differs, the host activates the session again with it. This reopens the open product.
- The host keeps the name with the wallet's public facts for the next sign-in. A full username is not passed on.
- With no record, `account.getUserId` answers `Unknown`. A failed or undecodable read changes nothing.

## Sessions

- A session belongs to the tab. Each tab runs its own core, so two tabs can use two wallets.
- A reload restores the tab's wallet from `sessionStorage`.
- A wallet switch reopens the product under the new session. A product runtime must not outlive its session.

## Storage

The same product opened with another wallet starts with that wallet's data.

| Data                                                         | Separation                                                                              |
| ------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| Core storage (grants, AutoSigning keys), TrUAPI storage      | Per wallet and product id, by `localStorage` key                                        |
| Mount path, service worker, product frame                    | Per wallet, product and content                                                         |
| Device encryption key                                        | Once per browser profile. The core needs it to outlive logout                           |
| Verified archive bytes                                       | Shared by content id, on purpose                                                        |
| A product frame's own `localStorage`, Cache Storage, workers | Chrome: `credentialless`, empty on each open. Other browsers: the host origin's, shared |
| Cookies and other origin-global state                        | Shared in every browser                                                                 |

- The browser does not separate paths on one origin. The host does not wrap storage APIs.
- A product can reach the host page's storage through `window.parent`.
- Keep a product's lasting data in TrUAPI storage.
