//! Emits `dispatcher.rs`: the server-side wire dispatcher that routes
//! incoming frames to the host trait implementation.
//!
//! For each method the emitter produces an `on_request` (or
//! `on_subscription`) registration that:
//! 1. SCALE-decodes the versioned request wrapper directly from the wire
//!    bytes (the wrapper's own variant tag carries its version — there is no
//!    outer envelope).
//! 2. Calls the host trait method (which receives the wrapper directly
//!    and matches `_::V1(inner)` internally).
//! 3. SCALE-encodes the versioned response wrapper back onto the wire.
//!
//! Which leg of the exchange a frame carries (a request's
//! request/response/cancel, or a subscription's start/receive/interrupt/stop)
//! is the outer wire's own `message_type` byte, addressed alongside
//! `(trait, method)` by the framework. This module never encodes or matches
//! on it. A request handler receives the cancellation token the framework
//! registered for that `requestId` and threads it into its `CallContext`, so a
//! `Cancel` frame reaches the trait method.
//!
//! The generated file expects to live inside a `truapi` crate
//! and references `crate::dispatcher::Dispatcher`. The codegen itself
//! does not compile the output; string-diff golden tests guard it.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::{Result, bail};
use indoc::{formatdoc, indoc, writedoc};

use crate::rustdoc::*;

use super::{const_name, module_for_trait, wire_method_name};

/// Emit the contents of `dispatcher.rs`.
pub fn generate_dispatcher(api: &ApiDefinition) -> Result<String> {
    let traits = order_traits(api)?;

    // Reject any duplicate wire method name across traits before emission, so
    // a future addition can't silently overwrite a handler in the HashMap.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for trait_def in &traits {
        for method in &trait_def.methods {
            let key = wire_method_name(&trait_def.name, &method.name);
            if !seen.insert(key.clone()) {
                bail!(
                    "Wire method name `{key}` registered twice; \
                     change `{}::{}` or its sibling trait to disambiguate",
                    trait_def.name,
                    method.name
                );
            }
        }
    }

    let mut modules = Vec::with_capacity(traits.len());
    for trait_def in &traits {
        modules.push(build_module(api, trait_def)?);
    }

    let mut out = String::new();
    write_header(&mut out);
    write_imports(&mut out, &traits);
    writeln!(out).unwrap();
    write_top_register(&mut out, &traits);
    write_host_initiated_callers(&mut out, api, &traits)?;

    for module in &modules {
        writeln!(out).unwrap();
        out.push_str(module);
    }

    Ok(out)
}

/// Returns the traits to emit, in the order declared by the top-level
/// `TrUApi` super-trait. Falls back to alphabetical order if the
/// extractor did not record a public ordering (e.g. synthetic tests).
fn order_traits(api: &ApiDefinition) -> Result<Vec<&TraitDef>> {
    let by_name: BTreeMap<&str, &TraitDef> =
        api.traits.iter().map(|t| (t.name.as_str(), t)).collect();

    if api.public_trait_order.is_empty() {
        return Ok(api.traits.iter().collect());
    }

    let mut ordered = Vec::with_capacity(api.public_trait_order.len());
    for name in &api.public_trait_order {
        let Some(trait_def) = by_name.get(name.as_str()) else {
            bail!("trait `{name}` appears in TrUApi but was not extracted");
        };
        ordered.push(*trait_def);
    }
    Ok(ordered)
}

/// Emit the `register_{module}` function for a single trait.
fn build_module(api: &ApiDefinition, trait_def: &TraitDef) -> Result<String> {
    let module = module_for_trait(&trait_def.name);

    let mut methods = Vec::with_capacity(trait_def.methods.len());
    for method in trait_def
        .methods
        .iter()
        .filter(|method| !method.wire.host_initiated)
    {
        let wire_method = wire_method_name(&trait_def.name, &method.name);
        methods.push(MethodEmission::build(
            api,
            &module,
            &wire_method,
            method,
            trait_def.required_execution(),
        )?);
    }

    let fn_name = format!("register_{module}");
    let trait_name = &trait_def.name;
    let mut code = String::new();
    writedoc!(
        code,
        r#"
        fn {fn_name}<P>(dispatcher: &mut Dispatcher, host: Arc<P>)
        where
            P: {trait_name} + Send + Sync + 'static,
        {{
        "#
    )
    .unwrap();
    let last = methods.len().saturating_sub(1);
    for (idx, method) in methods.iter().enumerate() {
        let host_expr = if idx == last { "host" } else { "host.clone()" };
        method.write(&mut code, host_expr)?;
    }
    writeln!(code, "}}").unwrap();

    Ok(code)
}

/// Emit the free functions that start a host-initiated subscription (a
/// method the host calls into the product, e.g.
/// `renderer_render`). Its `Start` payload is the request wrapper's own
/// encoding, sent immediately rather than registered against the dispatcher;
/// the product's `Receive`/`Interrupt` replies are routed back by
/// `HostInitiatedSubscriptionManager`.
fn write_host_initiated_callers(
    out: &mut String,
    api: &ApiDefinition,
    traits: &[&TraitDef],
) -> Result<()> {
    let wrappers = versioned_wrapper_names(api);
    for trait_def in traits {
        let module = module_for_trait(&trait_def.name);
        for method in trait_def
            .methods
            .iter()
            .filter(|method| method.wire.host_initiated)
        {
            let [request] = method.params.as_slice() else {
                bail!(
                    "Host-initiated method `{}` must have exactly one request parameter",
                    method.name
                );
            };
            let request_name = versioned_wrapper_root(
                &method.name,
                "host-initiated request",
                &request.type_ref,
                &wrappers,
            )?;
            let ReturnType::Subscription { item, interrupt } = &method.return_type else {
                bail!(
                    "Host-initiated method `{}` must return Subscription<Item, Interrupt>",
                    method.name
                );
            };
            let item_name =
                versioned_wrapper_root(&method.name, "host-initiated item", item, &wrappers)?;
            let interrupt_path = error_type_path(
                &module,
                &versioned_error_wrapper(&method.name, interrupt, &wrappers)?,
            );
            let wire_name = wire_method_name(&trait_def.name, &method.name);
            let ids = const_name(&wire_name);
            let request_path = format!("versioned::{module}::{request_name}");
            let item_path = format!("versioned::{module}::{item_name}");

            writedoc!(
                out,
                r#"

                /// Start the host-initiated `{wire_name}` subscription.
                pub fn {wire_name}(
                    subscriptions: &HostInitiatedSubscriptionManager,
                    transport: Arc<dyn Transport>,
                    request: {request_path},
                ) -> truapi::Subscription<{item_path}, truapi::CallError<{interrupt_path}>> {{
                    subscriptions.start(
                        wire_table::{ids},
                        parity_scale_codec::Encode::encode(&request),
                        transport,
                    )
                }}
                "#
            )
            .unwrap();
        }
    }
    Ok(())
}

struct MethodEmission {
    /// Rust method name on the host trait (used for the `host.<name>(...)` call).
    name: String,
    /// Fully-qualified wire method name (`{trait_snake}_{method}`); uppercased
    /// to the `wire_table` const this method registers against.
    wire_name: String,
    module: String,
    kind: MethodKind,
    /// The versioned wrapper naming this method's request payload.
    request_payload: String,
    response_wrapper: Option<String>,
    error_payload: String,
    item_wrapper: Option<String>,
    required_execution: Option<String>,
}

impl MethodEmission {
    fn build(
        api: &ApiDefinition,
        module: &str,
        wire_method: &str,
        method: &MethodDef,
        required_execution: Option<&str>,
    ) -> Result<Self> {
        let versioned_wrappers = versioned_wrapper_names(api);
        let request_payload = match method.params.as_slice() {
            [] => {
                bail!(
                    "Method `{}`: expected exactly one request parameter, so an empty request needs a \
                 payload-less versioned wrapper",
                    method.name
                )
            }
            [param] => {
                match &param.type_ref {
                    TypeRef::Named { name, args }
                        if args.is_empty() && versioned_wrappers.contains(name) =>
                    {
                        name.clone()
                    }
                    _ => {
                        bail!(
                            "Method `{}`: its request parameter is not a versioned wrapper, so it has no \
                     representable wire payload",
                            method.name
                        )
                    }
                }
            }
            _ => {
                bail!(
                    "Method `{}`: expected at most one request parameter (got {})",
                    method.name,
                    method.params.len()
                )
            }
        };
        let error_payload = match &method.return_type {
            ReturnType::Result { err, .. } => {
                versioned_error_wrapper(&method.name, err, &versioned_wrappers)?
            }
            ReturnType::Subscription { interrupt, .. } => {
                versioned_error_wrapper(&method.name, interrupt, &versioned_wrappers)?
            }
        };

        let (response_wrapper, item_wrapper) = match &method.return_type {
            ReturnType::Result { ok, .. } => {
                (
                    Some(
                        versioned_wrapper_root(&method.name, "response", ok, &versioned_wrappers)?
                            .to_string(),
                    ),
                    None,
                )
            }
            ReturnType::Subscription { item, .. } => {
                (
                    None,
                    Some(
                        versioned_wrapper_root(
                            &method.name,
                            "subscription item",
                            item,
                            &versioned_wrappers,
                        )?
                        .to_string(),
                    ),
                )
            }
        };

        Ok(MethodEmission {
            name: method.name.clone(),
            wire_name: wire_method.to_string(),
            module: module.to_string(),
            kind: method.kind,
            request_payload,
            response_wrapper,
            error_payload,
            item_wrapper,
            required_execution: required_execution.map(str::to_string),
        })
    }

    fn write(&self, out: &mut String, host_expr: &str) -> Result<()> {
        match self.kind {
            MethodKind::Request => self.write_request_envelope(out, host_expr),
            MethodKind::Subscription => self.write_subscription_envelope(out, host_expr),
        }
    }

    /// Generates a request/response handler. The incoming bytes decode
    /// directly as the method's request wrapper (its own variant tag is the
    /// version); the reply is `Result<{Response}, CallError<{Error}>>`,
    /// downgraded to the version the request wrapper carried.
    fn write_request_envelope(&self, out: &mut String, host_expr: &str) -> Result<()> {
        let module = &self.module;
        let method = &self.name;
        let ids = const_name(&self.wire_name);

        let request_name = &self.request_payload;
        let error_name = &self.error_payload;
        let request_path = format!("versioned::{module}::{request_name}");
        let error_path = format!("versioned::{module}::{error_name}");
        let response_path = self
            .response_wrapper
            .as_ref()
            .map(|name| format!("versioned::{module}::{name}"));
        let response_ty = response_path.as_deref().unwrap_or("()");

        writeln!(out, "    {{").unwrap();
        self.write_execution_binding(out);
        write_indented(
            out,
            8,
            &formatdoc! {
                r#"
                let host = {host_expr};
                dispatcher.on_request(wire_table::{ids}, move |request_id: String, bytes: Vec<u8>, cancel: truapi::CancellationToken| {{
                    let host = host.clone();
                    Box::pin(async move {{
                "#
            },
        );

        write_indented(
            out,
            16,
            &formatdoc! {
                r#"
                let request: {request_path} = match DecodeAll::decode_all(&mut &bytes[..]) {{
                    Ok(request) => request,
                    Err(err) => {{
                        let error: truapi::CallError<{error_path}> =
                            truapi::CallError::MalformedFrame {{ reason: err.to_string() }};
                        let result: Result<{response_ty}, truapi::CallError<{error_path}>> = Err(error);
                        return result.encode();
                    }}
                }};
                let target_version = request.version();
                let cx = CallContext::with_parts(request_id, cancel);
                "#
            },
        );

        if self.required_execution.is_some() {
            write_indented(
                out,
                16,
                &formatdoc! {
                    r#"
                    if !execution_allowed {{
                        let error: truapi::CallError<{error_path}> = truapi::CallError::Denied;
                        let result: Result<{response_ty}, truapi::CallError<{error_path}>> = Err(error);
                        return result.encode();
                    }}
                    "#
                },
            );
        }

        match &response_path {
            Some(response_path) => {
                write_indented(
                    out,
                    16,
                    &formatdoc! {
                        r#"
                        let result: Result<{response_path}, truapi::CallError<{error_path}>> =
                            match host.{method}(&cx, request).await {{
                                Ok(response) => Ok(<{response_path} as truapi::versioned::FromLatest>::from_latest(
                                    truapi::versioned::IntoLatest::into_latest(response),
                                    target_version,
                                )),
                                Err(err) => Err(downgrade_call_error(err, target_version)),
                            }};
                        result.encode()
                        "#
                    },
                );
            }
            None => {
                write_indented(
                    out,
                    16,
                    &formatdoc! {
                        r#"
                        let result: Result<(), truapi::CallError<{error_path}>> = match host.{method}(&cx, request).await {{
                            Ok(()) => Ok(()),
                            Err(err) => Err(downgrade_call_error(err, target_version)),
                        }};
                        result.encode()
                        "#
                    },
                );
            }
        }

        write_indented(
            out,
            4,
            indoc! {
                r#"
                        })
                    });
                }
                "#
            },
        );
        Ok(())
    }

    /// Generates a subscription handler. The incoming bytes decode directly
    /// as the method's request wrapper (its `Start` payload), or `()` for a
    /// method with no request parameter — its version then falls back to the
    /// item wrapper's latest, since no per-request signal exists to derive one
    /// from. Items are downgraded to that version and streamed as `Receive`
    /// frames. The stream's own terminating `Err`, and any failure before the
    /// stream exists, are encoded as the `Interrupt` payload's `Err` arm; a
    /// stream that ends without one interrupts with `Ok(())`, which the
    /// runtime encodes with no per-method type knowledge. `Stop` is
    /// intercepted by the framework before it ever reaches a registered
    /// handler.
    fn write_subscription_envelope(&self, out: &mut String, host_expr: &str) -> Result<()> {
        let module = &self.module;
        let method = &self.name;
        let ids = const_name(&self.wire_name);

        let Some(item_name) = self.item_wrapper.as_deref() else {
            bail!("Method `{method}`: subscription methods must have an item wrapper");
        };
        let item_path = format!("versioned::{module}::{item_name}");

        let request_name = &self.request_payload;
        let start_ty = format!("versioned::{module}::{request_name}");

        let error_ty = error_type_path(module, &self.error_payload);

        writeln!(out, "    {{").unwrap();
        self.write_execution_binding(out);
        write_indented(
            out,
            8,
            &formatdoc! {
                r#"
                let host = {host_expr};
                dispatcher.on_subscription(wire_table::{ids}, move |request_id: String, bytes: Vec<u8>| {{
                    let host = host.clone();
                    Box::pin(async move {{
                "#
            },
        );

        write_indented(
            out,
            16,
            &formatdoc! {
                r#"
                let request: {start_ty} = match DecodeAll::decode_all(&mut &bytes[..]) {{
                    Ok(request) => request,
                    Err(err) => {{
                        let error: truapi::CallError<{error_ty}> =
                            truapi::CallError::MalformedFrame {{ reason: err.to_string() }};
                        return Err(subscription_interrupt(error));
                    }}
                }};
                "#
            },
        );

        writeln!(
            out,
            "                let target_version = request.version();"
        )
        .unwrap();
        write_indented(
            out,
            16,
            "let cx = CallContext::with_request_id(request_id);\n",
        );

        if self.required_execution.is_some() {
            write_indented(
                out,
                16,
                &formatdoc! {
                    r#"
                    if !execution_allowed {{
                        let error: truapi::CallError<{error_ty}> = truapi::CallError::Denied;
                        return Err(subscription_interrupt(error));
                    }}
                    "#
                },
            );
        }

        let call_args = "&cx, request";

        writeln!(
            out,
            "                let stream = host.{method}({call_args}).await;"
        )
        .unwrap();

        // A domain error carries its own versions, so it is downgraded with the
        // items to the version the caller asked in.
        let downgrade_interrupt =
            "\n        .map_err(|error| downgrade_call_error(error, target_version))";
        write_indented(
            out,
            16,
            &formatdoc! {
                r#"
                let stream = futures::StreamExt::map(
                    stream,
                    move |item: Result<{item_path}, truapi::CallError<{error_ty}>>| {{
                        item.map(|item| {{
                            <{item_path} as truapi::versioned::FromLatest>::from_latest(
                                truapi::versioned::IntoLatest::into_latest(item),
                                target_version,
                            )
                        }}){downgrade_interrupt}
                    }},
                );
                Ok(subscription_stream(stream))
                "#
            },
        );

        write_indented(
            out,
            4,
            indoc! {
                r#"
                        })
                    });
                }
                "#
            },
        );
        Ok(())
    }

    fn write_execution_binding(&self, out: &mut String) {
        if let Some(required) = self.required_execution.as_ref() {
            writeln!(
                out,
                "        let execution_allowed = dispatcher.allows_execution(ProductExecutionKind::{required});"
            )
            .unwrap();
        }
    }
}

/// Resolve a method's error payload to its versioned wrapper. Every error the
/// wire carries has one; anything else has no representable payload.
fn versioned_error_wrapper(
    method: &str,
    ty: &TypeRef,
    versioned_wrappers: &BTreeSet<String>,
) -> Result<String> {
    let inner = call_error_inner(ty).unwrap_or(ty);
    versioned_wrapper_root(method, "error", inner, versioned_wrappers).map(ToString::to_string)
}

fn versioned_wrapper_root<'a>(
    method: &str,
    role: &str,
    ty: &'a TypeRef,
    versioned_wrappers: &BTreeSet<String>,
) -> Result<&'a str> {
    let TypeRef::Named { name, args } = ty else {
        bail!("Method `{method}`: {role} is not a versioned wrapper")
    };
    if !args.is_empty() || !versioned_wrappers.contains(name) {
        bail!("Method `{method}`: {role} is not a versioned wrapper")
    }
    Ok(name)
}

fn versioned_wrapper_names(api: &ApiDefinition) -> BTreeSet<String> {
    api.types
        .iter()
        .filter_map(|ty| {
            let TypeDefKind::Enum(variants) = &ty.kind else {
                return None;
            };
            if variants.iter().all(|variant| {
                variant
                    .name
                    .strip_prefix('V')
                    .is_some_and(|version| version.parse::<u32>().is_ok())
            }) {
                Some(ty.name.clone())
            } else {
                None
            }
        })
        .collect()
}

fn call_error_inner(ty: &TypeRef) -> Option<&TypeRef> {
    match ty {
        TypeRef::Named { name, args } if name == "CallError" && args.len() == 1 => Some(&args[0]),
        _ => None,
    }
}

/// Append `block` to `out`, prefixing every non-empty line with `indent` spaces.
fn write_indented(out: &mut String, indent: usize, block: &str) {
    let pad = " ".repeat(indent);
    for line in block.lines() {
        if line.is_empty() {
            out.push('\n');
        } else {
            writeln!(out, "{pad}{line}").unwrap();
        }
    }
}

fn write_header(out: &mut String) {
    writedoc!(
        out,
        r#"
        //! Wire dispatcher for the unified `TrUApi` trait.
        //!
        //! Auto-generated by truapi-codegen. Do not edit.

        // Responses are downgraded to the caller's version uniformly, including
        // the methods whose payload is unit and for which the conversion is a
        // no-op.
        #![allow(clippy::unit_arg)]

        "#
    )
    .unwrap();
}

/// Rust path of the versioned domain payload a method's `CallError` carries.
fn error_type_path(module: &str, error: &str) -> String {
    format!("versioned::{module}::{error}")
}

fn write_imports(out: &mut String, traits: &[&TraitDef]) {
    writedoc!(
        out,
        r#"
        use std::sync::Arc;

        use parity_scale_codec::{{DecodeAll, Encode}};

        use truapi::CallContext;
        use truapi::api::{{
        "#
    )
    .unwrap();
    for trait_def in traits {
        writeln!(out, "    {},", trait_def.name).unwrap();
    }
    writedoc!(
        out,
        r#"
        }};
        use truapi::versioned::{{self, Versioned}};
        use crate::platform::ProductExecutionKind;

        use crate::dispatcher::Dispatcher;
        use crate::frame::downgrade_call_error;
        use crate::generated::wire_table;
        use crate::subscription::{{
            HostInitiatedSubscriptionManager, subscription_interrupt, subscription_stream,
        }};
        use crate::transport::Transport;
        "#
    )
    .unwrap();
}

fn write_top_register(out: &mut String, traits: &[&TraitDef]) {
    writedoc!(
        out,
        r#"
        /// Register every TrUAPI method with the dispatcher.
        pub fn register<P>(dispatcher: &mut Dispatcher, host: Arc<P>)
        where
            P: truapi::api::TrUApi + 'static,
        {{
        "#
    )
    .unwrap();
    let last = traits.len().saturating_sub(1);
    for (idx, trait_def) in traits.iter().enumerate() {
        let host_expr = if idx == last { "host" } else { "host.clone()" };
        let module = module_for_trait(&trait_def.name);
        writeln!(out, "    register_{module}(dispatcher, {host_expr});").unwrap();
    }
    writeln!(out, "}}").unwrap();
}
