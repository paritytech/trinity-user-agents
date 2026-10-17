---
title: "Host username directory"
owner: "Leonardo Custodio"
status: draft
---

# RFC — Host username directory

## Summary

A product finds people by the start of their username, as the host's own contact search does, and asks the host to
add a person to the contacts of the app. The host asks the user first and adds the person its own way, so every product
of the user can then pick that person with `contacts.pick`.

## Motivation

A chat product that adds people by username has no search: the user must type the whole username, such as
`alice.42`, with no suggestions. The host has a username search for its own contacts, but no product can use it.

The contacts of a product are also its own. A person that the user added in one product is not a contact of the app,
so another product cannot pick that person, and the native chat of the app does not know them either.

## Approach

A new `Usernames` trait has two methods.

```ts
const found = await truapi.usernames.search({ prefix: "ali" });
assert(found.isOk(), "usernames.search failed:", found);
console.log(found.value.usernames); // ["alice", "alice.42"]

const added = await truapi.usernames.addContact({ username: "alice.42" });
assert(added.isOk(), "usernames.addContact failed:", added);
console.log(added.value.outcome); // "Added", "AlreadyContact", "Dismissed" or "NotFound"
```

```rust
pub struct HostUsernamesSearchRequest {
    /// 2 to 64 letters, digits, `.`, `-` or `_`, with no `@`.
    pub prefix: String,
}

pub struct HostUsernamesSearchResponse {
    /// At most 20 usernames, with no `@`, in the order of the host.
    pub usernames: Vec<String>,
}

pub enum HostUsernamesSearchError {
    NotConnected,
    InvalidRequest { reason: String },
    Unknown { reason: String },
}

pub struct HostUsernamesAddContactRequest { pub username: String }

pub enum AddContactOutcome { Added, AlreadyContact, Dismissed, NotFound }

pub struct HostUsernamesAddContactResponse { pub outcome: AddContactOutcome }

pub enum HostUsernamesAddContactError {
    NotConnected,
    /// The calling execution is not on screen, and is not a Worker handling a press.
    NotVisible,
    InvalidRequest { reason: String },
    Unknown { reason: String },
}

#[wire_trait(id = 27)]
pub trait Usernames {
    #[wire(id = 0)]
    async fn search(&self, cx: &CallContext, request: HostUsernamesSearchRequest)
        -> Result<HostUsernamesSearchResponse, CallError<HostUsernamesSearchError>>;

    #[wire(id = 1)]
    async fn add_contact(&self, cx: &CallContext, request: HostUsernamesAddContactRequest)
        -> Result<HostUsernamesAddContactResponse, CallError<HostUsernamesAddContactError>>;
}
```

| Answer           | What the product does next                                              |
| ---------------- | ----------------------------------------------------------------------- |
| `Added`          | Nothing: the host started its own add, a chat request for example.      |
| `AlreadyContact` | Nothing: the person is a contact of the app already.                    |
| `Dismissed`      | Can offer the add again.                                                |
| `NotFound`       | Tells the user that the network has no such username.                   |
| `NotVisible`     | Offers the add from its own screen, or from a press.                    |
| `InvalidRequest` | Fixes the call. Nothing was shown.                                      |
| `Unsupported`    | Asks for the whole username. This host has no directory.                |

**Usernames only.** A search gives usernames, never an account or a key. The core checks the answer of the host: it
drops what is not a username and repeats, and gives at most 20.

**The host owns the add.** The host asks the user, names the product and the username, and adds the person the way it
adds a contact itself. The product learns how the add ended, nothing more. Cancelling the call closes the question.

**Only for what the user is looking at.** An add asks the user, so it follows the rule of the share sheet: an App or
Widget only while it is the screen the user sees, a Worker only within 5 seconds of a press on something the host drew
for it, such as an entry of the menu of a participant. A search shows nothing and has no such rule.

**One question.** An add and a share sheet are both questions over the screen, so the core allows one of them at a
time.

**Hosts.** The iOS app searches with its chat contact search, which checks each username on chain, and adds a person
by opening its chat request screen. Android, the web hosts and the CLI host roles have no directory yet and answer
`Unsupported`.

## Trade-offs

- A search can tell a product which usernames exist. The host's own search tells anyone the same, and the core gives
  at most 20 per call.
- `Added` means the host started the add. On iOS the person is a contact after they accept the chat request, as for a
  contact that the user adds in the app.
- The product names the person by username. A product that knows a person only by a claimed username can name the
  wrong owner, so the host shows the username in its question.
