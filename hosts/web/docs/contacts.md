# Contacts

## Wallet contacts

The host keeps no chat of its own. It reads the wallet's saved chat record
(`truapi-web-signing-host:network:<id>:chat:<wallet>:v1`, `src/contacts/wallet-contacts.ts`).

- **Contacts** are the accepted, unblocked people in the record. Requests that either side has not accepted are not
  contacts. Nothing in this host adds a contact.
- The host offers contacts only when the wallet's derived keys match the identity account and chat key that the core
  reports. It never offers them for the Substrate development phrase. While Contact Chat uses the network delivery, it
  does not offer them either. So they are not payable then.
- **Writes.** The host writes the record only to block or unblock a contact (`blocked: true`). It changes and deletes
  nothing else.
- **Block** (Chat options, after a confirmation): the person is not told, leaves the picker, and the core is told, so
  their handles stop resolving. Their room stays.
- **Blocked**, under the chat list, lists blocked people with **Unblock**, and **Block a contact** (contacts only, no
  username search). Only a record contact can be blocked, so unblocking never makes a stranger payable.
- **Names** in Chat and Contact Chat (`src/contacts/host-names.ts`): the name chosen in the picker this session, then
  the record's names, then the chain username.

## The contact picker

The host serves TrUAPI Contacts (`src/contacts/contacts.ts`), as `docs/design/contacts-pick-end-to-end.md` describes.

- `contacts.pick` opens a host picker. The core turns the choice into a contact handle. The product gets only the
  handle: never the list, a name or an account. The choice is the consent.
- **Offered:** the wallet's contacts, most recent chat first, or a demo user's fixture contacts. Never people from
  product rooms, because a product decides who they are, and a contact can become a payment recipient.
- **Answers:** `Picked`, or `Dismissed` on Cancel, Escape or a tap outside. The host never answers `NoContacts`: an
  empty picker can still search by username.

### Username search

- When typing pauses, the host also looks up the text as a username (`src/contacts/username-directory.ts`).
- On a dev server on this machine: prefix search, 20 per page, through the
  [username search route](running.md#username-search-route). In a build: an exact on-chain read (Asset Hub and People).
  A demo user searches the other demo users.
- Results that are not contacts show under **Not in your contacts**.
- Before a pick, the host checks on chain that the username names the same account. The index is a listing, not a proof.
  A failed check picks nothing.
- The person is not saved. Text that cannot be a username is not sent. A new query or closing cancels the old one.

### Resolving handles

- For a transaction, the core asks the host which contact a handle hashes to (BLAKE2b-256 keyed with the session's
  handle key, over the account) and checks the answer.
- The host tells the core when a contact leaves or the session ends. Cached handles then stop resolving.
- A person picked by username is remembered in this page only. After a reload or session change, a transaction with that
  handle fails with `UnknownContact` until the product picks again.

### Not served

A readable contact list, labels for a handle, saving a person found by username, sending a request to a picked handle,
and contacts from a worker's own chat. Paying a picked contact uses the core's transaction path, which no browser test
reaches.
