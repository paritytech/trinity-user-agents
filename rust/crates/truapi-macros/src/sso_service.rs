//! Request/response conversions, wire helpers, and dispatch from SSO handlers.

use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{FnArg, ImplItem, ItemImpl, Signature, Type, parse_macro_input};

/// Parse the macro input and emit generated code or a compiler diagnostic.
pub fn expand(
    args: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    if !args.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "`sso_service` takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let item = parse_macro_input!(item as syn::Item);
    let result = match item {
        syn::Item::Impl(item) => expand_sso_service(item),
        other => Err(syn::Error::new_spanned(
            other,
            "sso_service requires an inherent implementation",
        )),
    };
    match result {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Pair and dispatch the handlers in one inherent implementation.
fn expand_sso_service(mut item: ItemImpl) -> syn::Result<TokenStream> {
    if item.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            &item,
            "sso_service requires an inherent implementation",
        ));
    }
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "SSO handlers require a concrete service type",
        ));
    }
    let wire = quote!(crate::host_internal::sso_wire);
    let message = quote!(crate::host_internal::sso_messages::v1::RemoteMessage);
    let runtime = quote!(crate::runtime::sso_service);
    let reply = quote!(#runtime::SsoReply);
    let mut impls = Vec::new();
    let mut arms = Vec::new();
    let mut name_arms = Vec::new();
    let mut responses = Vec::new();
    for entry in &mut item.items {
        let ImplItem::Fn(method) = entry else {
            return Err(syn::Error::new_spanned(
                entry,
                "the annotated implementation holds only SSO handler methods",
            ));
        };
        let (request_ty, response_ty) = method_types(&method.sig)?;
        method.sig.output = syn::parse_quote!(-> #reply<#response_ty>);
        let body = &method.block;
        method.block = syn::parse_quote!({
            (async move #body).await.into()
        });
        let response_variant = last_segment(&response_ty)?;
        if !responses.contains(&response_variant) {
            responses.push(response_variant.clone());
        }
        let name = &method.sig.ident;
        let method_name = name.unraw().to_string();
        let stem: String = method_name
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect();
        let variant = format_ident!("{stem}Request", span = name.span());
        impls.push(quote! {
            impl #wire::SsoRequest for #request_ty {
                const NAME: &'static str = #method_name;
                type Response = #response_ty;

                fn response_into_message(
                    response: crate::host_internal::sso_messages::Response<Self::Response>,
                ) -> #message {
                    #message::#response_variant(response)
                }

                fn response_from_message(
                    message: #message,
                ) -> Option<crate::host_internal::sso_messages::Response<Self::Response>> {
                    match message {
                        #message::#response_variant(response) => Some(response),
                        _ => None,
                    }
                }

                fn into_message(self) -> #message {
                    #message::#variant(self)
                }
            }
        });
        arms.push(quote! {
            #message::#variant(request) => {
                let reply = match &cx {
                    Some(cx) => self.#name(cx, request).await,
                    None => Err(#wire::SsoError::not_connected()).into(),
                };
                reply.finish(&message_id, <#request_ty as #wire::SsoRequest>::response_into_message)
            }
        });
        name_arms.push(quote! { Self::#variant(_) => #method_name });
    }
    if arms.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "the annotated implementation declares no SSO handlers",
        ));
    }

    let mut responding_to_arms = Vec::new();
    let mut retarget_arms = Vec::new();
    for variant in responses {
        let name = variant.to_string();
        arms.push(quote! {
            #message::#variant(_) => return #runtime::Dispatch::NotARequest(#name),
        });
        name_arms.push(quote! { Self::#variant(_) => #name });
        responding_to_arms.push(quote! {
            Self::#variant(response) => Some(&response.responding_to),
        });
        retarget_arms.push(quote! {
            Self::#variant(mut response) => {
                response.responding_to = responding_to;
                Self::#variant(response)
            }
        });
    }

    item.items.push(syn::parse_quote! {
        /// Answer one wire message with this service.
        ///
        /// `cx` is the context for `message`, built by the caller around the
        /// transport's bound wallet session; without one every request is
        /// answered with its error type's `not_connected()`.
        pub async fn dispatch(
            &self,
            cx: Option<#runtime::SsoRequestContext>,
            message: crate::host_internal::sso_messages::RemoteMessage,
        ) -> #runtime::Dispatch {
            let crate::host_internal::sso_messages::RemoteMessageData::V1(data) = message.data;
            let message_id = message.message_id;
            let answer = match data {
                #message::Disconnected => return #runtime::Dispatch::Disconnected,
                #message::Cancel(withdrawal) => {
                    return #runtime::Dispatch::Withdraw(withdrawal.message_id)
                }
                #(#arms)*
            };
            #runtime::Dispatch::Response(Box::new(answer))
        }
    });
    Ok(quote! {
        #item
        #(#impls)*

        impl #message {
            /// Service method name for requests; variant name for other messages.
            pub fn name(&self) -> &'static str {
                match self {
                    Self::Disconnected => "Disconnected",
                    Self::Cancel(_) => "Cancel",
                    #(#name_arms,)*
                }
            }

            /// Request id answered by a response; `None` for other messages.
            pub fn responding_to(&self) -> Option<&str> {
                match self {
                    #(#responding_to_arms)*
                    _ => None,
                }
            }

            /// Re-address a response; other messages pass through unchanged.
            pub fn with_responding_to(self, responding_to: String) -> Self {
                match self {
                    #(#retarget_arms,)*
                    other => other,
                }
            }
        }
    })
}

fn method_types(sig: &Signature) -> syn::Result<(Type, Type)> {
    let mut inputs = sig.inputs.iter();
    let shape_error = || {
        syn::Error::new_spanned(
            sig,
            "expected `async fn name(&self, cx: &SsoRequestContext, request: <Request>) -> <Response>`",
        )
    };
    let Some(FnArg::Receiver(receiver)) = inputs.next() else {
        return Err(shape_error());
    };
    if receiver.reference.is_none()
        || receiver.mutability.is_some()
        || receiver.colon_token.is_some()
    {
        return Err(shape_error());
    }
    let Some(FnArg::Typed(context)) = inputs.next() else {
        return Err(shape_error());
    };
    let Type::Reference(context_ty) = context.ty.as_ref() else {
        return Err(shape_error());
    };
    if last_segment(&context_ty.elem)? != "SsoRequestContext" {
        return Err(shape_error());
    }
    let Some(FnArg::Typed(request)) = inputs.next() else {
        return Err(shape_error());
    };
    if inputs.next().is_some()
        || sig.asyncness.is_none()
        || !sig.generics.params.is_empty()
        || sig.generics.where_clause.is_some()
    {
        return Err(shape_error());
    }
    let syn::ReturnType::Type(_, output) = &sig.output else {
        return Err(shape_error());
    };
    let Type::Path(path) = output.as_ref() else {
        return Err(shape_error());
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(shape_error());
    };
    if !segment.ident.to_string().ends_with("Response") {
        return Err(shape_error());
    }
    Ok(((*request.ty).clone(), output.as_ref().clone()))
}

fn last_segment(ty: &Type) -> syn::Result<Ident> {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.clone())
            .ok_or_else(|| syn::Error::new_spanned(ty, "expected a request type")),
        _ => Err(syn::Error::new_spanned(ty, "expected a request type path")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_requires_an_explicit_wire_response() {
        let item = syn::parse_quote! {
            impl Service {
                async fn sign(&self, cx: &SsoRequestContext, request: SignRequest)
                    -> Result<Vec<u8>, String> { Ok(vec![]) }
            }
        };
        let error = expand_sso_service(item).unwrap_err();
        assert_eq!(
            error.to_string(),
            "expected `async fn name(&self, cx: &SsoRequestContext, request: <Request>) -> <Response>`"
        );
    }
}
