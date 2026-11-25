---
title: "Product Database"
owner: "@tommyldev"
status: draft
---

# RFC — Product Database

## Summary

A host-owned SQLite database per product and account, exposed as the `Database` trait: a product runs SQL in transactions it begins, commits and rolls back, sets a schema version in the same commit as its migration, and hears about every commit. The trait is the surface an SQL library's driver needs, so a product brings its own query builder and migrator. Each database is its own SQLite file, so SQLite's own per-file limits meter it, and a product that outgrows its quota asks the user for more. The database is local to the device; replication is a follow-up RFC.

## Motivation

Product state lives in `LocalStorage` and, where the webview offers it, IndexedDB. `LocalStorage` is a flat key-value map with no listing, no batch and no atomic update, so a product that keeps a history rewrites whole JSON documents per change and maintains its own index keys. The Desktop backend applies each write as a read-modify-write of the whole product store, so concurrent writes drop each other's keys. IndexedDB is missing from some product webviews and from the QuickJS worker runtime, so a worker cannot share structured state with its webview.

t3ams (team chat over Statement Store) submits statements with a 24-hour expiry, so the device is the only durable copy of a conversation. It needs to page through a channel's history, search it, and record a received message together with the bookkeeping that says it was received, in one step that either happens or does not.

### Requirements

1. **Queryable.** A product reads a bounded, ordered subset of its rows without loading the rest, and joins and searches across tables.
2. **Transactional.** A product reads and writes inside one transaction that commits whole or not at all, and is durable on the device when the commit returns.
3. **Migratable.** A product changes its schema and rewrites its data in one transaction, keyed on a schema version the host stores.
4. **Observable.** A product's webview and worker learn of each other's commits without polling.
5. **Bounded.** One product cannot exhaust the device's storage or hold its database locked.

## Approach

The host keeps one SQLite database per product per signed-in account. With no account signed in the product gets the anonymous database ([Unauthenticated Product Access](0009-unauthenticated-product-access.md)), and its rows do not move into the account's database at sign-in. An executable is one of the product's App, Widget or Worker, as the [Product Manifest](product-manifest.md) defines them.

The design has eight parts:

- **Transactions**: how a product runs SQL.
- **Migrations**: how a product changes its schema.
- **Changes**: how a product observes commits.
- **Storage**: how the host lays out database files and bounds their cost.
- **Limits**: what bounds one product.
- **Lifecycle**: what happens to the database around sign-in and removal.
- **Isolation**: how the host keeps one product out of another's database.
- **Trait**: the wire surface.

### Transactions

`begin` opens a read or a write transaction and returns its id and the schema version. `execute` runs a list of SQL statements with positional parameters inside that transaction and returns, per statement, the rows, the number of rows changed and the last inserted row id. `commit` makes the writes durable, `rollback` discards them. `execute` without a transaction id runs in a transaction of its own that commits when the call returns, so a single write is one call.

Each `execute` call runs inside a savepoint. A statement that fails rolls back that call and leaves the transaction open, so the product decides whether to retry, carry on or roll back. A product may open savepoints of its own. A read transaction rejects a statement that writes.

A database has at most one write transaction at a time; a write `begin`, or an `execute` without a transaction, waits for the current one to end. Each executable holds at most one write transaction, so a second write `begin` from the same executable fails with `Busy` instead of waiting on itself. An `execute` without a transaction runs as a read transaction when every statement reads, otherwise as a write transaction under the same rule. A read transaction sees a snapshot and never waits for a writer.

A transaction id is valid only for the executable that began it. The host rolls a transaction back on `rollback`, when no call arrives within the idle timeout, when the executable ends, and when the signed-in account changes. A later call on its id fails with `TransactionClosed`.

The host admits `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `REPLACE` and `WITH`; `CREATE`, `ALTER` and `DROP` of tables, indexes, views and triggers, including FTS5 virtual tables; savepoints inside an open transaction; `PRAGMA foreign_keys` and `PRAGMA defer_foreign_keys`; and reads of `sqlite_master` and `pragma_table_info`, which a library's introspection uses. Everything else fails with `Rejected`, in particular every statement that would reach outside the product's database or around the trait: `ATTACH`, every other `PRAGMA`, `VACUUM`, `TEMP` objects, `BEGIN`, `COMMIT` and `ROLLBACK`, extension loading, and writes to `sqlite_` tables. Hosts ship SQLite 3.45.0 or later with FTS5 and the JSON functions.

### Migrations

The host stores one schema version per database, starting at 0. `begin` reports it and `commit` sets it, so a migration's DDL, its data rewrite and the new version commit together or not at all. The host does not interpret the version.

Migrations run on the product side. A library migrator keeps its own migrations table in the database and runs the pending steps in one write transaction; its adapter sets the host's version in that commit, so the host's version is authoritative for every product. A product without a library begins a write transaction, compares the reported version with the one its code expects, runs its steps and commits with the new version. A stored version above the one the code expects means a rolled-back release, and the product refuses to open rather than downgrade.

Because only one write transaction runs at a time, a webview and a worker that open the database together do not both migrate: the second waits for the first to commit, reads the state the first left, and has nothing to do.

### Changes

`changes_subscribe` emits one item per committed write transaction that changed any of the tables the subscription names, or any table when it names none. The item carries the tables the transaction changed and the tag its `begin` or `execute` carried, so an executable recognises its own commits. The item names tables, not rows; the product re-reads what it shows. A product that needs a row-level log writes one to a table of its own in the same transaction. When the signed-in account changes, every open subscription ends with `AccountChanged`.

### Storage

Each database is one SQLite file in WAL mode, named as [Isolation](#isolation) describes. The host does not multiplex products into one shared file, for these reasons:

- **Library compatibility.** A product's SQL runs unmodified and `sqlite_master` lists only the product's own tables, so a library's migrator and introspection work as they do against any SQLite file. A shared file needs every table name rewritten per product in every statement, and `sqlite_master`, `pragma_table_info` and FTS5's shadow tables filtered. The authorizer can refuse a statement but cannot rewrite it, so the host would need its own SQL rewriter, and every case it misses breaks a library or exposes another product's table.
- **Isolation.** The boundary between products is the file, as [Isolation](#isolation) describes, not a naming convention inside one.
- **Writers.** SQLite runs one write transaction per file. In a shared file every product's writes queue behind one lock, so one product catching up on a day of messages makes every other product's writes wait past the write-wait limit. With a file each, one product's open transaction never blocks another.
- **Metering.** SQLite bounds a file's size with `max_page_count` and fails the write that would pass it, which is exact and costs nothing. A shared file needs the host to count each product's pages itself, either by scanning the file with `dbstat`, which is too slow to run per write, or by keeping a running total, which drifts with indexes, FTS5 and free pages.
- **Removal.** Removing a product or an account deletes files. In a shared file the space returns only after a `VACUUM`, which rewrites the whole file and holds every product's writes until it finishes.
- **Fault containment.** A corrupt file or a growing write-ahead log affects one product.

Each open database costs a writer connection, reader connections, a page cache per connection, and three open files: the database, its write-ahead log and its shared-memory index. The host keeps that cost proportional to the databases in use, not the products installed:

- It opens a database on the first call that needs it.
- It runs every database's connections on one bounded worker pool shared by all products, not a pool per database. The core's own store ([#963](https://github.com/paritytech/host-rust-core/issues/963)) gives its single database one writer and a read pool; product databases share threads instead.
- It gives each database at most one pool thread for its writer and a bounded share for its readers, so a product cannot take the whole pool.
- It caps the page cache of each connection.
- It closes a database that has no open transaction and no open subscription after an idle period, and caps the number of open databases, closing the least recently used idle one first.
- It checkpoints the write-ahead log and bounds the log's size after a checkpoint with `journal_size_limit`.

The pool size, cache size, idle period and open-database cap are host configuration, not part of the contract. A product sees one effect: the first call after the host closed its database pays for opening it, including recovery of its write-ahead log. The host never closes a database that has an open transaction or subscription.

Requests from different products never wait on each other's locks. Each file has its own write lock, so two products write at the same time, and a slow transaction in one product delays only that product. What products share is the pool: a product running long statements, each bounded by the statement run time, uses at most its share of threads and slows only itself. Several databases are open at once wherever several products run: products, widgets and workers side by side on desktop and in the browser host, and on mobile the foreground product together with the workers that draw [Pocket](pocket-modality.md) cards. Within one product, its App, Widget and Worker share one database under the single-writer rule in [Transactions](#transactions).

The trait never names a file, so a host can change the layout later, for example to multiplex products at scale, without a wire change, provided it keeps every guarantee in this RFC.

### Limits

The host guarantees every product at least these floors on every platform and may allow more. Each limit applies per database. `stats` reports the values the host applies, so a product reads them rather than assuming the floors.

| Limit | Floor | Enforced by | When exceeded |
| --- | --- | --- | --- |
| Database size | 64 MiB | SQLite: `PRAGMA max_page_count`, which the host sets on every connection it opens | The statement fails with `Full`; the call rolls back and the transaction stays open so the product can delete rows. |
| Request size of one `execute`, SQL text and parameters together | 1 MiB | Host, before it prepares a statement | The call fails with `TooLarge`, carrying the host's maximum; nothing runs. |
| One text or blob value, including one a statement builds | 1 MiB | SQLite: `sqlite3_limit` with `SQLITE_LIMIT_LENGTH` | The statement fails with `TooLarge`; the call rolls back and the transaction stays open. |
| Result size of one `execute` | 4 MiB | Host, while it steps the rows | The call fails with `TooLarge` and rolls back; the product pages with `LIMIT`. |
| Run time of one statement | 5 s | Host, through `sqlite3_progress_handler` | The host interrupts the statement, which fails with `Interrupted`; the call rolls back and the transaction stays open. |
| Idle time between calls on a transaction | 10 s | Host | The host rolls the transaction back; the next call on it fails with `TransactionClosed`. |
| Wait for the write transaction | 5 s | Host, which queues write transactions | The call fails with `Busy`; nothing runs. |
| Open transactions per executable | 1 write, 4 read | Host | `begin` fails with `Busy`. |

The database size counts the pages the file holds. The write-ahead log beside it is bounded by checkpoints, not by the quota. A write also fails with `Full` below the quota when the device runs out of space. The host never deletes rows to make room: a full database still serves reads and deletes, and accepts writes again once the product frees pages or its quota rises.

A product that needs more than its quota requests `DatabaseQuota` through `ResourceAllocation` ([RFC 0010](0010-allowance.md)) with the total size it needs. TrUAPI defines the request and its outcome; how the host decides is the host's own, as RFC 0010 leaves its authorization UI to the Account Holder. A host may prompt on each request, grant in fixed steps, offer the user a size control, grant automatically up to a budget it sets, or refuse, and it decides on the device without the Account Holder. It answers `Allocated` once the quota covers the requested size, `Rejected` when the user or the host declines, and `NotAvailable` when the device cannot hold it. A grant may exceed the request, and `stats` reports the quota that results. The host keeps a grant for the product and account until the user changes it.

The host may also lower a quota, for example when the user does. A database above its quota keeps its rows, serves reads and deletes, and fails writes that need new pages with `Full`. The host never deletes a product's rows and never moves one product's quota to another, as [Scheduled Notifications](0019-scheduled-notifications.md) never evicts another product's entry to make room.

### Lifecycle

The trait needs no permission prompt; only raising the quota goes through the host's approval. A host that does not implement it does not register it, so every call fails with `Unsupported` and the product falls back to `LocalStorage`. Sign-out keeps the account's database; removing the account from the device deletes it. Removing the product from the device deletes every database of that product.

### Isolation

SQLite has no users, roles or schemas, so the boundary between products is the file. The host binds the product id to the connection when it accepts it, as it does for `PermissionsService`, and derives the file name from that id under the state root of the signed-in account; no request names a database. SQLite runs in the host process and the product only sends SQL text: the statement policy is enforced with `sqlite3_set_authorizer` at prepare time, so a comment or a CTE cannot hide an `ATTACH`, and each connection opens with SQLite's settings for untrusted SQL. Transaction ids and subscriptions are looked up under the product and executable that created them, so an id from anywhere else fails with `TransactionClosed`.

### Trait

Trait id 20 follows `Worker`; ids are append-only.

```rust
//! Unified [`Database`] trait.

use crate::versioned::database::{
    HostDatabaseBeginError, HostDatabaseBeginRequest, HostDatabaseBeginResponse,
    HostDatabaseChangesSubscribeError, HostDatabaseChangesSubscribeItem,
    HostDatabaseChangesSubscribeRequest, HostDatabaseCommitError, HostDatabaseCommitRequest,
    HostDatabaseCommitResponse, HostDatabaseExecuteError, HostDatabaseExecuteRequest,
    HostDatabaseExecuteResponse, HostDatabaseRollbackError, HostDatabaseRollbackRequest,
    HostDatabaseRollbackResponse, HostDatabaseStatsError, HostDatabaseStatsRequest,
    HostDatabaseStatsResponse,
};
use crate::{CallContext, CallError, Subscription};
use crate::{wire, wire_trait};

/// SQLite database scoped to the calling product and the signed-in account.
#[wire_trait(id = 20)]
#[crate::async_trait]
pub trait Database: Send + Sync {
    /// Open a read or a write transaction.
    ///
    /// ```ts
    /// const tx = await truapi.database.begin({ mode: { tag: "Write" }, tag: undefined });
    /// assert(tx.isOk(), "begin failed:", tx);
    /// console.log("transaction:", tx.value.transaction, "schema version:", tx.value.schemaVersion);
    /// ```
    #[wire(id = 0)]
    async fn begin(
        &self,
        _cx: &CallContext,
        _request: HostDatabaseBeginRequest,
    ) -> Result<HostDatabaseBeginResponse, CallError<HostDatabaseBeginError>> {
        Err(CallError::unavailable())
    }

    /// Run SQL statements inside a transaction, or in one of their own.
    ///
    /// ```ts
    /// const result = await truapi.database.execute({
    ///   transaction: undefined,
    ///   tag: undefined,
    ///   statements: [{
    ///     sql: "SELECT id, body FROM messages WHERE channel = ?1 ORDER BY ts DESC LIMIT 50",
    ///     params: [{ tag: "Text", value: "c1" }],
    ///   }],
    /// });
    /// assert(result.isOk(), "execute failed:", result);
    /// console.log("rows:", result.value.results[0].rows);
    /// ```
    #[wire(id = 1)]
    async fn execute(
        &self,
        _cx: &CallContext,
        _request: HostDatabaseExecuteRequest,
    ) -> Result<HostDatabaseExecuteResponse, CallError<HostDatabaseExecuteError>> {
        Err(CallError::unavailable())
    }

    /// Commit a transaction, optionally setting the schema version.
    ///
    /// ```ts
    /// const result = await truapi.database.commit({ transaction: 1n, schemaVersion: undefined });
    /// assert(result.isOk(), "commit failed:", result);
    /// ```
    #[wire(id = 2)]
    async fn commit(
        &self,
        _cx: &CallContext,
        _request: HostDatabaseCommitRequest,
    ) -> Result<HostDatabaseCommitResponse, CallError<HostDatabaseCommitError>> {
        Err(CallError::unavailable())
    }

    /// Discard a transaction's writes.
    ///
    /// ```ts
    /// const result = await truapi.database.rollback({ transaction: 1n });
    /// assert(result.isOk(), "rollback failed:", result);
    /// ```
    #[wire(id = 3)]
    async fn rollback(
        &self,
        _cx: &CallContext,
        _request: HostDatabaseRollbackRequest,
    ) -> Result<HostDatabaseRollbackResponse, CallError<HostDatabaseRollbackError>> {
        Err(CallError::unavailable())
    }

    /// Stream one item per committed write transaction.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const change = await firstValueFrom(
    ///   from(truapi.database.changesSubscribe({ request: { tables: ["messages"] } })),
    /// );
    /// console.log("changed:", change.tables, "tag:", change.tag);
    /// ```
    #[wire(id = 4)]
    async fn changes_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostDatabaseChangesSubscribeRequest,
    ) -> Subscription<HostDatabaseChangesSubscribeItem, CallError<HostDatabaseChangesSubscribeError>>
    {
        Subscription::interrupted(CallError::unavailable())
    }

    /// Report the bytes the database uses, its quota and the limits the host applies.
    ///
    /// ```ts
    /// const stats = await truapi.database.stats();
    /// assert(stats.isOk(), "stats failed:", stats);
    /// console.log("bytes used:", stats.value.bytesUsed, "quota:", stats.value.quota, "limits:", stats.value.limits);
    /// ```
    #[wire(id = 5)]
    async fn stats(
        &self,
        _cx: &CallContext,
        _request: HostDatabaseStatsRequest,
    ) -> Result<HostDatabaseStatsResponse, CallError<HostDatabaseStatsError>> {
        Err(CallError::unavailable())
    }
}
```

Each method's error is its own `V1` envelope over `DatabaseError`; `stats` uses `GenericError`. `HostDatabaseStatsRequest` is a payload-less `V1` envelope, `HostDatabaseCommitResponse` and `HostDatabaseRollbackResponse` are `()`, and `HostDatabaseChangesSubscribeItem` wraps `DatabaseChange`. Types, derives omitted:

```rust
/// Request to open a transaction.
pub struct HostDatabaseBeginRequest {
    /// Whether the transaction may write.
    pub mode: DatabaseTransactionMode,
    /// Bytes echoed on the change item the transaction's commit emits.
    pub tag: Option<Vec<u8>>,
}

/// Kind of transaction.
pub enum DatabaseTransactionMode {
    /// Reads a snapshot and never waits for a writer.
    Read,
    /// Holds the database's single write transaction.
    Write,
}

/// Response to opening a transaction.
pub struct HostDatabaseBeginResponse {
    /// Transaction id, valid for the executable that began it.
    pub transaction: u64,
    /// Schema version the database holds, 0 when never set.
    pub schema_version: u32,
}

/// Request to run SQL statements.
pub struct HostDatabaseExecuteRequest {
    /// Transaction to run in, or absent to run in a transaction of their own that commits on return.
    pub transaction: Option<u64>,
    /// Bytes echoed on the change item, used only when `transaction` is absent.
    pub tag: Option<Vec<u8>>,
    /// Statements run in order inside one savepoint.
    pub statements: Vec<DatabaseStatement>,
}

/// One SQL statement.
pub struct DatabaseStatement {
    /// A single statement with positional parameters.
    pub sql: String,
    /// Values bound to the positional parameters in order.
    pub params: Vec<DatabaseValue>,
}

/// Response to running SQL statements.
pub struct HostDatabaseExecuteResponse {
    /// One result per statement, in order.
    pub results: Vec<DatabaseStatementResult>,
}

/// Result of one statement.
pub struct DatabaseStatementResult {
    /// Result column names, empty for a statement that returns no rows.
    pub columns: Vec<String>,
    /// Result rows, one value per column.
    pub rows: Vec<Vec<DatabaseValue>>,
    /// Rows the statement inserted, updated or deleted.
    pub rows_changed: u64,
    /// Row id of the last row the statement inserted, absent when it inserted none.
    pub last_insert_rowid: Option<i64>,
}

/// A SQLite value.
pub enum DatabaseValue {
    /// SQL NULL.
    Null,
    /// 64-bit signed integer.
    Integer(i64),
    /// 64-bit float as its IEEE-754 bit pattern.
    Real(u64),
    /// UTF-8 text.
    Text(String),
    /// Raw bytes.
    Blob(Vec<u8>),
}

/// Request to commit a transaction.
pub struct HostDatabaseCommitRequest {
    /// Transaction to commit.
    pub transaction: u64,
    /// Schema version to store with the commit, or absent to keep the stored one.
    pub schema_version: Option<u32>,
}

/// Request to roll a transaction back.
pub struct HostDatabaseRollbackRequest {
    /// Transaction to roll back.
    pub transaction: u64,
}

/// Request to stream committed changes.
pub struct HostDatabaseChangesSubscribeRequest {
    /// Tables to observe; every table when empty.
    pub tables: Vec<String>,
}

/// One committed write transaction.
pub struct DatabaseChange {
    /// Tables the transaction changed.
    pub tables: Vec<String>,
    /// Tag the transaction carried.
    pub tag: Option<Vec<u8>>,
}

/// Database statistics.
pub struct HostDatabaseStatsResponse {
    /// Bytes in use: the database's pages minus its free pages, times the page size.
    pub bytes_used: u64,
    /// Bytes the host allows the database.
    pub quota: u64,
    /// Limits the host applies to the database.
    pub limits: DatabaseLimits,
}

/// Limits the host applies, each at or above its floor in the limits table.
pub struct DatabaseLimits {
    /// Largest `execute` request, SQL text and parameters together, in bytes.
    pub max_request_bytes: u64,
    /// Largest text or blob value, in bytes.
    pub max_value_bytes: u64,
    /// Largest `execute` result, in bytes.
    pub max_result_bytes: u64,
    /// Longest run time of one statement, in milliseconds.
    pub statement_timeout_ms: u32,
    /// Longest idle time between calls on a transaction, in milliseconds.
    pub idle_timeout_ms: u32,
    /// Longest wait for the write transaction, in milliseconds.
    pub write_wait_ms: u32,
    /// Most read transactions one executable holds open.
    pub max_read_transactions: u32,
}

/// Database operation error.
pub enum DatabaseError {
    /// SQLite rejected the statement, for example a syntax error or an unknown table.
    Sql {
        /// Index of the failing statement in the call.
        statement: u32,
        /// SQLite's error message.
        reason: String,
    },
    /// A statement violated a constraint.
    Constraint {
        /// Index of the failing statement in the call.
        statement: u32,
        /// SQLite's error message.
        reason: String,
    },
    /// The statement policy refused the statement, or a read transaction ran a write.
    Rejected {
        /// Index of the refused statement in the call.
        statement: u32,
        /// Human-readable rejection reason.
        reason: String,
    },
    /// The host interrupted a statement that exceeded its run time.
    Interrupted {
        /// Index of the interrupted statement in the call.
        statement: u32,
    },
    /// The request, one value or the result exceeds the host's size limit.
    TooLarge {
        /// Largest size the host accepts, in bytes.
        max_bytes: u64,
    },
    /// The database reached its quota, or the device ran out of space.
    Full,
    /// The write transaction stayed taken past the wait limit, or the executable holds its maximum of open transactions.
    Busy,
    /// The transaction was rolled back or committed, or belongs to another executable.
    TransactionClosed,
    /// The signed-in account changed.
    AccountChanged,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}
```

`AllocatableResource` in `ResourceAllocation` gains one variant, appended after the existing ones:

```rust
pub enum AllocatableResource {
    // Existing variants unchanged.
    /// Database quota of at least this many bytes for the product's database under the signed-in account.
    DatabaseQuota(u64),
}
```

## Trade-offs

- The database does not replicate across the account's devices. A follow-up RFC covers replication; the per-account database gives it a unit to replicate.
- An open write transaction holds the database across round trips between product and host. The idle timeout and the single write transaction per executable bound the cost. Considered and dropped: atomic batches of structured writes with compare-and-set, which cannot check rows they do not write or run a migration atomically.
- The SQLite dialect and its minimum version are part of the contract. Considered and dropped: structured write operations, which cannot express `UPDATE … WHERE` or `INSERT … SELECT`.
- Migrations live on the product side; the host only stores a version. Considered and dropped: a declarative schema the host diffs, which cannot rewrite data, and a host-side migration hook, which needs the host to call into product code.
- Isolation rests on a file per product and the SQLite authorizer, not on cryptography. A product's data is as private as the host's own storage, which is what `LocalStorage` gives today.
- Each product's database is its own file, so the host's cost grows with the databases open at once, which the host bounds by sharing threads and closing idle databases; a product pays an open on its first call after its database closes. Considered and dropped: one file multiplexing every product, which needs the host to rewrite product SQL, breaks library migrators and introspection, queues every product behind one writer, and meters only by counting pages itself. A host may revisit it at scale behind the same trait.
- The quota is a hard limit the host raises on request, not a budget the host reclaims, and the approval UI is the host's. Considered and dropped: a fixed approval flow in the protocol, which would bind every host to one UI across desktop, browser and mobile.
- The quota is never reclaimed by deleting data. Considered and dropped: evicting a product's data under storage pressure, as browsers do, which loses the only copy of data such as t3ams' history.
- Change notifications name tables, not rows. Considered and dropped: a row-level change feed, which needs a change log the host retains.
- `LocalStorage` is unchanged.

## Open questions

- The pool size, page cache size, idle period and open-database cap for mobile hosts, and the memory and open latency of one open database, which need measuring on devices.
- The largest row a transport frame carries. A product that caches media of tens of megabytes may need a streamed blob call rather than a `Blob` column.
- How the browser host persists a SQLite database.
- Whether the host enables foreign key enforcement. A migrator that rebuilds a table by copy, drop and rename inside a transaction cannot turn enforcement off there, so the drop of a referenced table runs enforced.
