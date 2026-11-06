---
title: "Composable Core"
owner: "@johnthecat"
status: draft
---

# RFC — Composable Core

## Summary

The `truapi` crate becomes composable. Each part that only some hosts need, or that depends on the platform, becomes an
extension point. The core declares it as a trait, and a provider crate in this repository may implement it. A host build
attaches the providers it needs. It can implement an extension point itself, or place its own code between a provider
and the core. That code can show UI for requests sent to the paired phone, or connect the core to a sandbox in a Web
Worker. The first three extension points are the account holder, the host session and the worker runtime. The core drops
`wasm-signing-host` and the other Cargo features that select a host role.

## Motivation

The `truapi` crate varies its builds with Cargo features, but hosts now need to select parts of the core at runtime:

- A host that runs in more than one role, such as a remote host with a debug signing mode, ships one core build per
  role. Each build repeats the code the roles share, which bloats the host.
- A host may need a platform-specific layer to manage its workers, and the
  [core-owned worker lifecycle](https://github.com/paritytech/trinity-user-agents/pull/1278) leaves no place for it.

A host also cannot put its own code around a part compiled into the core. Polkadot Web and Polkadot Desktop, both remote
hosts, cannot show the user the status of a request sent to the paired phone
([#1552](https://github.com/paritytech/trinity-user-agents/issues/1552)). The core requests resource allocation, ring
VRF keys and more from the phone without involving the host. Even where the host approves an operation through a
callback, it cannot follow its progress, pending state or errors.

## Approach

The design has two parts:

- **Extension points**: how the core declares and exports them.
- **Composition**: how a host build attaches providers to the core.

### Extension points

An extension point is a trait the core declares as a public interface, plus the logic that integrates its implementation
into the core. This repository may ship a recommended implementation as a provider crate, and any host can implement its
own. A core feature that only some hosts need, or that depends on the platform, is an extension point, never a Cargo
feature of the core.

- Each extension point and the types its methods name live in a small interface crate, which the core re-exports as the
  public module `host::extension`.
- Swift and Kotlin implement the trait directly through UniFFI. On the web, the core declares a matching JS interface
  and wraps an object implementing it in a type that implements the trait.
- A provider crate depends only on the interface crate, never on the core. A native host links the core and its
  providers into one binary. On the web, the core and each provider are separate WASM binaries, which the host connects
  at runtime through the core's JS interface.
- The core takes each provider as an `Arc<dyn Trait>` when a host runtime is built. So every extension point trait is
  object safe, and the runtime types the core exports through UniFFI and wasm-bindgen stay non-generic and in the core.

### Composition

On iOS, Android and the CLI, the host builds its library from a Rust crate of its own project, which links the core and
its providers. That crate has one constructor per exported host runtime, which builds the providers and passes them to
the core. On the web, TypeScript code does the same at runtime: it loads the core and each provider binary and passes
the providers to the core. `@parity/truapi-host` ships that code with one entry per host runtime, and a host adds its
own code around it.

A host may place its own code between the core and a provider. It writes that code in Rust in its own crate, or in
Swift, Kotlin or TypeScript against the exported extension point. That code implements the extension point and forwards
to the provider, so it sees every call the core makes through it and can add its own UI, state and calls.
`@parity/truapi-host` replaced
[`host-container` and `host-papp`](https://github.com/paritytech/triangle-js-sdks/tree/main/packages), which composed a
host the same way. For example, code around the [remote account holder](#account-holder) can show the status of each
request the core sends to the phone.

## Proposed solution

An imaginary extension point, `Notifier`, shows how each crate looks and how a web, iOS or Android host attaches a
provider. Each code block starts with the crate or app it lives in.

### Interface crate

The interface crate, `truapi-host-extension`, declares the trait and the types its methods name. The core re-exports it
as `host::extension::Notifier`. Every extension point error has one variant for failures the trait does not declare, and
the error's `From<String>` builds it. The UniFFI conversion calls it on native, and the generated code calls it on the
web with a rejected promise's reason.

```rust
// Interface: "truapi-host-extension" crate, "src/lib.rs"

#[cfg(not(target_arch = "wasm32"))]
uniffi::setup_scaffolding!();

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::{ForeignNotifier, WasmNotifier};

/// Why a notice was not shown.
#[derive(Debug, thiserror::Error)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Error))]
pub enum NotifyError {
	/// The platform or the host refused to show it.
	#[error("{reason}")]
	Rejected {
		/// Human-readable reason.
		reason: String,
	},
}

/// What a failure the trait does not declare becomes.
impl From<String> for NotifyError {
	fn from(reason: String) -> Self {
		Self::Rejected { reason }
	}
}

/// What an undeclared exception thrown by a Swift or Kotlin implementation becomes.
#[cfg(not(target_arch = "wasm32"))]
impl From<uniffi::UnexpectedUniFFICallbackError> for NotifyError {
	fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
		error.reason.into()
	}
}

/// Shows a notice to the user.
#[cfg_attr(not(target_arch = "wasm32"), uniffi::export(rust, foreign))]
#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
	/// Shows `text` and resolves once the notice is on screen.
	async fn notify(&self, text: String) -> Result<(), NotifyError>;
}
```

### Provider crate

The provider crate, `truapi-notifier-log`, depends only on the interface crate. `src/notifier.rs` implements the trait,
each export sits in its own file, and the crate root, `src/lib.rs`, holds only the export setup. `src/wasm.rs` returns
the provider as the `WasmNotifier` class that `truapi-codegen` generates in the interface crate. The crate's WASM build
is published as `@parity/truapi-notifier-log`. `src/native.rs` returns the provider as a trait object for UniFFI.

```rust
// Provider: "truapi-notifier-log" crate, "src/lib.rs"

#[cfg(not(target_arch = "wasm32"))]
uniffi::setup_scaffolding!();

mod notifier;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use notifier::LogNotifier;
```

```rust
// Provider: "truapi-notifier-log" crate, "src/notifier.rs"

use truapi_host_extension::{Notifier, NotifyError};

/// Notifier that writes each notice to the log.
pub struct LogNotifier;

#[async_trait::async_trait]
impl Notifier for LogNotifier {
	async fn notify(&self, text: String) -> Result<(), NotifyError> {
		tracing::info!("notice: {text}");
		Ok(())
	}
}
```

```rust
// Provider: "truapi-notifier-log" crate, "src/wasm.rs"

use std::sync::Arc;

use truapi_host_extension::WasmNotifier;
use wasm_bindgen::prelude::*;

use crate::LogNotifier;

/// The log notifier, for web host code to pass to the core or wrap.
#[wasm_bindgen(js_name = logNotifier)]
pub fn log_notifier() -> WasmNotifier {
	WasmNotifier::new(Arc::new(LogNotifier))
}
```

```rust
// Provider: "truapi-notifier-log" crate, "src/native.rs"

use std::sync::Arc;

use truapi_host_extension::Notifier;

use crate::LogNotifier;

/// The log notifier, for the app to pass to the runtime or wrap.
#[uniffi::export]
pub fn log_notifier() -> Arc<dyn Notifier> {
	Arc::new(LogNotifier)
}
```

### Generated web interface

`truapi-codegen` generates the web side of each extension point from its trait, as it generates the `HostCallbacks`
interface from the `platform` traits. For `Notifier`, it generates two files:

- `src/generated/extensions.ts` in `@parity/truapi-host`: the TypeScript `Notifier` interface.
- `src/wasm.rs` in `truapi-host-extension`: two wrapper types. `ForeignNotifier` wraps a JS object implementing the
  interface as a `Notifier` for the core. `WasmNotifier` is the wasm-bindgen class that exports a Rust provider to JS.
  It sits behind the crate's `export` Cargo feature, which only providers enable, so the core's WASM binary leaves it
  out.

```ts
// Core: "@parity/truapi-host" package, "src/generated/extensions.ts", generated by "truapi-codegen"

/** Shows a notice to the user. */
export interface Notifier {
  /** Shows `text` and resolves once the notice is on screen. */
  notify(text: string): Promise<void>;
}
```

### JS host

Host code loads both binaries and passes the provider to the core: the app loads and wraps the provider, and
`createHostRuntime` loads the core. `@parity/truapi-host/js` replaces the `/web` entry and exports `createHostRuntime`.
`callbacks` is the host's implementation of `HostCallbacks`, through which the core reaches storage, navigation and the
other platform services:

```ts
// JS host: app code. "@parity/truapi-host" and "@parity/truapi-notifier-log" come from this repository.

import { createHostRuntime, type Notifier } from "@parity/truapi-host/js";
import initLogNotifier, { logNotifier } from "@parity/truapi-notifier-log";

import { callbacks } from "./callbacks";
import { statusBar } from "./status-bar";

await initLogNotifier();
const inner = logNotifier();
const notifier: Notifier = {
  async notify(text) {
    statusBar.show(text);
    await inner.notify(text);
  },
};
const runtime = await createHostRuntime({ callbacks, notifier });
```

### iOS and Android hosts

On iOS and Android, the host crate links the core and the provider into one library and exports the constructor through
UniFFI. The library also carries the provider's UniFFI export:

```rust
// Host: "host-core" crate in the host project, "src/lib.rs". Built into the XCFramework or the Android AAR.

use std::sync::Arc;

use truapi::host::extension::Notifier;
use truapi::{HostCallbacks, HostRuntime};
// Links the provider's UniFFI export into the library.
use truapi_notifier_log as _;

uniffi::setup_scaffolding!();

/// Builds the host runtime with the callbacks and the notifier the app passes in.
#[uniffi::export]
pub fn host_runtime(callbacks: Arc<dyn HostCallbacks>, notifier: Arc<dyn Notifier>) -> Arc<HostRuntime> {
	HostRuntime::new(callbacks, notifier)
}
```

The app wraps the provider in Swift:

```swift
// iOS host: app code. "HostCore" is the Swift package built from the host crate.

import HostCore

final class StatusNotifier: Notifier {
    private let inner: Notifier

    init(inner: Notifier) {
        self.inner = inner
    }

    func notify(text: String) async throws {
        StatusBar.shared.show(text)
        try await inner.notify(text: text)
    }
}

let runtime = hostRuntime(callbacks: callbacks, notifier: StatusNotifier(inner: logNotifier()))
```

And in Kotlin:

```kotlin
// Android host: app code. The packages are the UniFFI bindings of the host crate's library.

import uniffi.host_core.hostRuntime
import uniffi.truapi_notifier_log.logNotifier
import uniffi.truapi_host_extension.Notifier

class StatusNotifier(private val inner: Notifier) : Notifier {
    override suspend fun notify(text: String) {
        statusBar.show(text)
        inner.notify(text)
    }
}

val runtime = hostRuntime(callbacks, StatusNotifier(logNotifier()))
```

## Trade-offs

- An extension point is a contract between crates: changing it changes every provider.
- CI in this repository builds the core and each provider separately. Each host builds and tests its own combination.
- On the web, every call between the core and a provider crosses a WASM boundary through JS, and each provider binary
  carries its own copy of the interface crate.
- A native host builds its library from its own Rust crate, so it needs the Rust toolchain. The prebuilt XCFramework and
  Android AAR this repository releases do not.
- Considered and dropped: a core generic over its providers. UniFFI and wasm-bindgen do not export generic types, so
  each host crate would have to wrap every runtime type it exports.

## Open questions

- Whether a host crate exports the core and its providers under the single `truapi` UniFFI namespace, or each provider
  crate keeps its own namespace that Swift and Kotlin import separately. The examples above use the second option.

## Examples

### Account holder

The account holder split leaves the account holder as the only difference between a signing host and a remote host. Both
run one `HostAccounts<H>`, where `H` is the wallet itself or a proxy to the phone's wallet over SSO. Once the split
lands, `AccountHolder` and `HostSession` become extension points. Each side's implementations of both move into a crate
of their own, with the services only that side uses. The core keeps `HostAccounts`, `ProductConsent` and grant storage.
The SSO wire protocol moves out of the core into its own crate, which both account holder providers use.

Each account holder provider, what it carries, and the host builds that link it:

| Provider              | Carries                                                                                                        | Linked by                    |
| --------------------- | -------------------------------------------------------------------------------------------------------------- | ---------------------------- |
| Wallet account holder | `WalletAccountHolder`, its `HostSession`, the SSO responder and per-peer services, statement allowance renewal | iOS, Android, CLI, test host |
| Remote account holder | `RemoteAccountHolder`, its `HostSession` over the paired phone and the SSO request service                     | Desktop, Web, CLI, test host |

### Worker runtime

The core keeps the worker manager. It counts the references consumers hold to each worker, keeps the worker's stored
state, applies bundle updates and serves the calls a worker makes into the core. The core has no sandbox and no
platform-specific worker code. The worker manager runs worker code through `WorkerRuntime`, which starts a worker from a
bundle with a channel to the core, and stops it.

The QuickJS runtime is the first provider crate. A native host attaches it to the core directly. Polkadot Web, Polkadot
Desktop and other JS hosts build it to WASM, load it in a Web Worker, and carry the channel between the core and the Web
Worker in their own TypeScript implementation of `WorkerRuntime`. A host with its own sandbox implements `WorkerRuntime`
itself. A host that attaches no worker runtime runs no workers. Calls that need one fail as unavailable, as calls
without platform backing do.
