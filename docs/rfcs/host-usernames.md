---
title: "Host username directory"
owner: "Leonardo Custodio"
status: draft
---

# RFC — Host username directory

## Summary

A product finds people by the start of their username, as the host's own contact search does.

## Motivation

A chat product that adds people by username has no search: the user must type the whole username, such as
`alice.42`, with no suggestions. The host has a username search for its own contacts, but no product can use it.

## Approach

A new `Usernames` trait has one method.

```ts
const found = await truapi.usernames.search({ prefix: "ali" });
assert(found.isOk(), "usernames.search failed:", found);
console.log(found.value.usernames); // ["alice", "alice.42"]
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

#[wire_trait(id = 27)]
pub trait Usernames {
    #[wire(id = 0)]
    async fn search(&self, cx: &CallContext, request: HostUsernamesSearchRequest)
        -> Result<HostUsernamesSearchResponse, CallError<HostUsernamesSearchError>>;
}
```

| Answer           | What the product does next                                              |
| ---------------- | ----------------------------------------------------------------------- |
| `InvalidRequest` | Fixes the call.                                                         |
| `Unsupported`    | Asks for the whole username. This host has no directory.                |

**Usernames only.** A search gives usernames, never an account or a key. The core checks the answer of the host: it
drops what is not a username and repeats, and gives at most 20.

**No question.** A search shows nothing to the user, so any execution may search.

**Hosts.** The iOS app searches with its chat contact search, which checks each username on chain. Android, the web
hosts and the CLI host roles have no directory yet and answer `Unsupported`.

## Trade-offs

- A search can tell a product which usernames exist. The host's own search tells anyone the same, and the core gives
  at most 20 per call.
