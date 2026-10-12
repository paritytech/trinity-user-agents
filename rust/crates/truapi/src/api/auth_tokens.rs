//! Unified [`AuthTokens`] trait.

use crate::versioned::auth_tokens::{
    HostAuthTokensGetError, HostAuthTokensGetRequest, HostAuthTokensGetResponse,
    HostAuthTokensRemoveError, HostAuthTokensRemoveRequest, HostAuthTokensRemoveResponse,
    HostAuthTokensStoreError, HostAuthTokensStoreRequest, HostAuthTokensStoreResponse,
};
use crate::{CallContext, CallError};
use crate::{wire, wire_trait};

/// Tokens a product obtained by signing in to its own services, kept by the
/// host.
///
/// The product signs in and refreshes its tokens itself; the host only keeps
/// them, in the platform's secure storage, so any of the product's executions
/// can read them back, including after a restart. Each product sees only its
/// own tokens, and the host drops them all when the user signs out. A product
/// without a session is answered `Denied`, and a host that cannot keep tokens
/// `Unsupported`.
#[wire_trait(id = 26)]
#[crate::async_trait]
pub trait AuthTokens: Send + Sync {
    /// Keep a token for one of the product's services, replacing any already
    /// kept for it.
    ///
    /// ```ts
    /// const result = await truapi.authTokens.store({
    ///   service: "onramp",
    ///   token: {
    ///     token: "eyJhbGciOiJIUzI1NiJ9.e30.signature",
    ///     refreshToken: "refresh-123",
    ///     expiresAt: BigInt(Date.now() + 3_600_000),
    ///   },
    /// });
    /// assert(result.isOk(), "store failed:", result);
    /// console.log("token stored");
    /// ```
    #[wire(id = 0)]
    async fn store(
        &self,
        cx: &CallContext,
        request: HostAuthTokensStoreRequest,
    ) -> Result<HostAuthTokensStoreResponse, CallError<HostAuthTokensStoreError>>;

    /// Read the token kept for one of the product's services.
    ///
    /// ```ts
    /// const result = await truapi.authTokens.get({ service: "onramp" });
    /// assert(result.isOk(), "get failed:", result);
    /// console.log("token kept:", result.value.token !== undefined);
    /// ```
    #[wire(id = 1)]
    async fn get(
        &self,
        cx: &CallContext,
        request: HostAuthTokensGetRequest,
    ) -> Result<HostAuthTokensGetResponse, CallError<HostAuthTokensGetError>>;

    /// Drop the token kept for one of the product's services. Dropping one
    /// that is not kept succeeds.
    ///
    /// ```ts
    /// const result = await truapi.authTokens.remove({ service: "onramp" });
    /// assert(result.isOk(), "remove failed:", result);
    /// console.log("token removed");
    /// ```
    #[wire(id = 2)]
    async fn remove(
        &self,
        cx: &CallContext,
        request: HostAuthTokensRemoveRequest,
    ) -> Result<HostAuthTokensRemoveResponse, CallError<HostAuthTokensRemoveError>>;
}
