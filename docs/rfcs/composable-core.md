---
title: "Composable Core"
owner: "@johnthecat"
status: draft
---

# RFC — Composable Core

## Summary

The `truapi` crate becomes composable. Everything a host supplies to the core is an extension point: a trait declared in
an interface crate. Each part that only some hosts need, or that depends on the platform, is one, and so is each
platform service the host supplies through `HostCallbacks` today. A provider crate in this repository may implement an
extension point. Each host builds one crate of its own that links the core and the providers it uses into one binary.
The web host does so too. The host can implement an extension point itself, or place its own code between a provider and
the core. That code can show UI for requests sent to the paired phone, or connect the core to a sandbox in a Web Worker.
The account holder and the host session become the first new extension points, and the worker runtime follows. The core
drops `wasm-signing-host` and the other Cargo features that select a host role. Products are unchanged: they reach the
core only through protocol frames.

A [sketch](https://github.com/johnthecat/composable-core-sketch) implements the design end to end on truapi's
architecture. Its web, Swift and Kotlin hosts all drive the core with a stand-in product.

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
callback, it cannot follow the operation's progress or see its errors.

Adding a host callback for each such case does not scale. Each callback serves only some hosts, the remote ones here,
and the core has to foresee and maintain every one. With the core split into extension points, the host puts whatever
code it needs between the core and a provider, and the core declares only the extension point.

## Approach

The design has two parts:

- **Extension points**: how the core declares them and exports them to each platform.
- **Composition**: how a host build attaches providers to the core.

### Extension points

An extension point is a trait the core declares as a public interface, plus the logic that integrates its implementation
into the core. This repository may ship a recommended implementation as a provider crate, and any host can implement its
own. A core feature that only some hosts need, or that depends on the platform, is an extension point, never a Cargo
feature of the core.

- Each extension point is a public module of the interface crate, `truapi-host-extension`, such as
  `truapi_host_extension::notifier`. The core re-exports the crate as `truapi::host::extension`. The module holds:
  - the trait;
  - its error, `<Trait>Error`;
  - the types the trait names;
  - on `wasm32`, its web glue.
- The protocol definitions move out of `truapi` into a crate of their own, `truapi-protocol`. The interface crate
  depends on it and never on the core, so an extension point names protocol types, such as `ProductAccount`, as they
  are. A protocol record an extension point carries is marked `#[protocol_record]`, which adds the attributes
  `#[extension_type]` adds.
- A provider crate depends only on the interface crate.
- Attributes from `truapi-macros` write each extension point's UniFFI export and web glue. `#[extension_point]` marks
  the trait, `#[extension_error]` its error and `#[extension_type]` each record or enum it names. `truapi-codegen`
  writes only the TypeScript interfaces, from the rustdoc JSON of the interface crate and the protocol crate.
- Values are plain data, never live objects, and cross to JS through serde. Bytes are the protocol's byte types, such as
  `Bytes` and `Bytes32`, which reach JS as a `Uint8Array`. The attributes refuse `Vec<u8>` and `[u8; N]`, which would
  reach JS as an array of numbers.
- A method the core calls is `async fn(&self, owned values) -> Result<T, <Trait>Error>`. No call stays pending to
  deliver changes. State the core follows has a plain read, and its changes are pushed to a listener the core hands over
  once through `fn set_<value>_listener(&self, Arc<dyn Listener>)`. A listener carries `#[extension_listener]`, and its
  methods return nothing.
- `<Trait>Error` lists only the failures the trait declares. `#[extension_error]` adds an `Unknown { reason }` variant
  for every other failure, such as an exception a Swift implementation throws without declaring it. An error is a type
  of its trait's crate, because UniFFI's Kotlin bindings cannot lower another crate's error from a foreign trait's async
  method.
- The core takes each extension point as an `Arc<dyn Trait>` when a host runtime is built. So every extension point
  trait is object safe, and the runtime types the core exports through UniFFI and wasm-bindgen stay non-generic. An
  optional extension point is an `Option`, and a product call that needs a missing one fails as `Unsupported`.
- Natively the core calls every extension point from one executor thread, and on the web from the page's thread. An
  async method suspends rather than blocks, and a method that returns nothing hands its value on and returns at once.
- A product runtime owns its calls in flight, so dropping it drops them. The core dropping a call cancels a Swift or
  Kotlin implementation's task. A JS promise cannot be cancelled, so a JS implementation runs on and the core discards
  its result.

### Composition

Each host builds its library from a Rust crate of its own project, which links the core and its providers into one
binary. On the web that binary is the host's one WASM binary. The host crate declares one constructor per exported host
runtime, with one parameter per extension point. `#[truapi::constructor]` exports it through UniFFI natively and as a
wasm-bindgen function on the web, where each parameter is typed with the generated TypeScript interface. The binary also
carries each provider's own export, which returns the provider for host code to pass to the constructor or wrap.

A host may place its own code between the core and a provider. It writes that code in Rust in its own crate, or in
Swift, Kotlin or TypeScript against the exported extension point. That code implements the extension point and forwards
to the provider, so it sees every call the core makes through it. It can add its own UI and state, and make calls of its
own. A wrapper that follows pushed changes gives the provider its own listener and forwards to the core's.
`@parity/truapi-host` replaced
[`host-container` and `host-papp`](https://github.com/paritytech/triangle-js-sdks/tree/main/packages), which composed a
host the same way. For example, code around the [remote account holder](#account-holder) can show the status of each
request the core sends to the phone.

The host frames each product as it does today. For each product it passes the product's `ProductContext` and a
`FrameSink`, which are an extension type and an extension listener of the interface crate, and sends the product's
frames into the product runtime.

## Proposed solution

An imaginary extension point, `Notifier`, shows how each crate looks and how a web, iOS or Android host attaches a
provider. Each code block starts with the crate or app it lives in.

### Interface crate

The interface crate declares the trait, its error and the types its methods name. The core re-exports the module as
`truapi::host::extension::notifier`.

```rust
// Interface: "truapi-host-extension" crate, "src/notifier.rs"

//! The `Notifier` extension point: notices shown to the user.

use truapi_macros::{extension_error, extension_point};

/// Why a notice was not shown.
#[extension_error]
pub enum NotifierError {
	/// The platform or the host refused to show it.
	#[error("refused: {reason}")]
	Rejected {
		/// Human-readable reason.
		reason: String,
	},
}

/// Shows a notice to the user.
#[extension_point]
pub trait Notifier: Send + Sync {
	/// Shows `text` and resolves once the notice is on screen.
	async fn notify(&self, text: String) -> Result<(), NotifierError>;
}
```

`#[extension_error]` adds the `Unknown { reason }` variant with its `From<String>`. It derives `thiserror::Error`, along
with the UniFFI and serde attributes `#[extension_type]` adds. `#[extension_point]` adds the UniFFI export of a trait
Swift and Kotlin implement, and on `wasm32` three types:

- `JsNotifier`, a JS object typed as the TypeScript `Notifier` interface;
- `ForeignNotifier`, which wraps such an object as a `Notifier` for the core;
- `WasmNotifier`, which hands a Rust provider to JS with `WasmNotifier::into_js`.

On the web, a declared error rejects as an `Error` named `NotifierException` whose `error` property holds it. Any other
rejection becomes `Unknown`. The crate root names the TypeScript package the interfaces are generated into, once:

```rust
// Interface: "truapi-host-extension" crate, "src/lib.rs"

#[cfg(not(target_arch = "wasm32"))]
uniffi::setup_scaffolding!();

truapi_macros::typescript_module!("@parity/truapi-host");

pub mod notifier;
```

### Provider crate

The provider crate, `truapi-notifier-log`, depends only on the interface crate. `src/notifier.rs` implements the trait.
Each export sits in its own file, and the crate root, `src/lib.rs`, holds only the export setup. `src/native.rs` returns
the provider as a trait object for UniFFI, and `src/wasm.rs` returns it as a `JsNotifier`.

```rust
// Provider: "truapi-notifier-log" crate, "src/lib.rs"

#[cfg(not(target_arch = "wasm32"))]
uniffi::setup_scaffolding!();

#[cfg(not(target_arch = "wasm32"))]
mod native;
mod notifier;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use notifier::LogNotifier;
```

```rust
// Provider: "truapi-notifier-log" crate, "src/notifier.rs"

use truapi_host_extension::notifier::{Notifier, NotifierError};

/// Notifier that writes each notice to the log.
pub struct LogNotifier;

#[async_trait::async_trait]
impl Notifier for LogNotifier {
	async fn notify(&self, text: String) -> Result<(), NotifierError> {
		tracing::info!("notice: {text}");
		Ok(())
	}
}
```

```rust
// Provider: "truapi-notifier-log" crate, "src/native.rs"

use std::sync::Arc;

use truapi_host_extension::notifier::Notifier;

use crate::LogNotifier;

/// The log notifier, for the app to pass to the runtime or wrap.
#[uniffi::export]
pub fn log_notifier() -> Arc<dyn Notifier> {
	Arc::new(LogNotifier)
}
```

```rust
// Provider: "truapi-notifier-log" crate, "src/wasm.rs"

use std::sync::Arc;

use truapi_host_extension::notifier::{JsNotifier, WasmNotifier};
use wasm_bindgen::prelude::*;

use crate::LogNotifier;

/// The log notifier, for web host code to pass to the runtime or wrap.
#[wasm_bindgen(js_name = logNotifier)]
pub fn log_notifier() -> JsNotifier {
	WasmNotifier::into_js(Arc::new(LogNotifier))
}
```

### Host crate

A native host's crate links the core and the provider into one library, and declares the constructor once. The `use` of
the provider links its export into the library. The web host's crate is the same, built for `wasm32` only and without
the UniFFI scaffolding:

```rust
// Host: "host-core" crate in the iOS or Android host project, "src/lib.rs"

use std::sync::Arc;

use truapi::host::extension::{confirmation, notifier, storage};
use truapi::{HostExtensions, HostRuntime};
// Links the provider's export into the library.
use truapi_notifier_log as _;

uniffi::setup_scaffolding!();

/// Builds the host runtime over the extension points the app passes in.
/// Without a notifier, notices fail as `Unsupported`.
#[truapi::constructor]
pub fn host_runtime(
	product_storage: Arc<dyn storage::ProductStorage>,
	user_confirmation: Arc<dyn confirmation::UserConfirmation>,
	notifier: Option<Arc<dyn notifier::Notifier>>,
) -> Arc<HostRuntime> {
	HostRuntime::new(HostExtensions { product_storage, user_confirmation, notifier })
}
```

### Generated web interface

`truapi-codegen` writes the TypeScript side of each extension point into `@parity/truapi-host`:

```ts
// Core: "@parity/truapi-host" package, "src/generated/extensions.ts", generated by "truapi-codegen"

/** Shows a notice to the user. */
export interface Notifier {
  /** Shows `text` and resolves once the notice is on screen. */
  notify(text: string): Promise<void>;
}
```

The constructor's `.d.ts` imports these interfaces from the package named in `typescript_module!`:

```ts
// Host: the WASM binary of "host-core", "host_core.d.ts", generated by wasm-bindgen

import type * as HostExtension from "@parity/truapi-host";

export function hostRuntime(
  product_storage: HostExtension.ProductStorage,
  user_confirmation: HostExtension.UserConfirmation,
  notifier?: HostExtension.Notifier | null,
): HostRuntime;
export function logNotifier(): HostExtension.Notifier;
```

### Web host

Host code loads the host crate's binary, wraps the provider and passes it to the constructor. The constructor throws,
naming the method, when an object lacks one:

```ts
// Web host: app code. "host-core" is the WASM binary built from the host crate.

import type { Notifier } from "@parity/truapi-host";

import init, { hostRuntime, logNotifier } from "./host-core/host_core.js";
import { productStorage, userConfirmation } from "./platform";
import { statusBar } from "./status-bar";

await init();
const inner = logNotifier();
const notifier: Notifier = {
  async notify(text) {
    statusBar.show(text);
    await inner.notify(text);
  },
};
const runtime = hostRuntime(productStorage, userConfirmation, notifier);
```

### iOS and Android hosts

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

let runtime = hostRuntime(
    productStorage: productStorage,
    userConfirmation: userConfirmation,
    notifier: StatusNotifier(inner: logNotifier())
)
```

And in Kotlin, where each crate's bindings are a package of their own:

```kotlin
// Android host: app code. The packages are the UniFFI bindings of the host crate's library.

import uniffi.host_core.hostRuntime
import uniffi.truapi_host_extension.Notifier
import uniffi.truapi_notifier_log.logNotifier

class StatusNotifier(private val inner: Notifier) : Notifier {
    override suspend fun notify(text: String) {
        statusBar.show(text)
        inner.notify(text)
    }
}

val runtime = hostRuntime(productStorage, userConfirmation, StatusNotifier(logNotifier()))
```

## Trade-offs

- An extension point is a contract between crates: changing it changes every provider.
- CI in this repository builds the core and each provider separately. Each host builds and tests its own combination.
- Every host builds its binary from its own Rust crate, so it needs the Rust toolchain, the web host included.
- Each crate is its own UniFFI namespace. Swift compiles the bindings of a library into one module, and Kotlin gets a
  package per crate.
- Considered and dropped: a core generic over its providers. UniFFI and wasm-bindgen do not export generic types, so
  each host crate would have to wrap every runtime type it exports.
- Considered and dropped: one WASM binary per provider on the web. Each would carry its own copy of the interface crate,
  and every call between the core and a provider would cross a WASM boundary through JS.

## Open questions

- Whether this repository releases a prebuilt binary for each standard host, so a host that needs no providers of its
  own skips the Rust toolchain.

## Examples

### Account holder

The account holder split leaves the account holder as the only difference between a signing host and a remote host. Both
run one `HostAccounts<H>`, where `H` is the wallet itself or a proxy to the phone's wallet over SSO. Once the split
lands, `AccountHolder` and `HostSession` become extension points. Each side's implementations of both move into a crate
of their own, with the services only that side uses. The core keeps `HostAccounts` and `ProductConsent`. Grant storage
stays in the core too. The SSO wire protocol moves out of the core into its own crate, which both account holder
providers use.

The account holder providers:

| Provider              | Carries                                                                                                        | Linked by                    |
| --------------------- | -------------------------------------------------------------------------------------------------------------- | ---------------------------- |
| Wallet account holder | `WalletAccountHolder`, its `HostSession`, the SSO responder and per-peer services, statement allowance renewal | iOS, Android, CLI, test host |
| Remote account holder | `RemoteAccountHolder`, its `HostSession` over the paired phone and the SSO request service                     | Desktop, Web, CLI, test host |

### Worker runtime

The core keeps the worker manager, which:

- counts the references consumers hold to each worker;
- keeps the worker's stored state;
- applies bundle updates;
- serves the calls a worker makes into the core.

The core has no sandbox and no platform-specific worker code. The worker manager runs worker code through
`WorkerRuntime`, which starts a worker from a bundle with a channel to the core, and stops it.

The QuickJS runtime is the first provider crate. A native host attaches it to the core directly. A JS host, such as
Polkadot Web or Polkadot Desktop, builds it to WASM and loads it in a Web Worker. The host carries the channel between
the core and the Web Worker in its own TypeScript implementation of `WorkerRuntime`. A host with its own sandbox
implements `WorkerRuntime` itself. A host that attaches no worker runtime runs no workers. Calls that need one fail as
unavailable, as calls without platform backing do.
