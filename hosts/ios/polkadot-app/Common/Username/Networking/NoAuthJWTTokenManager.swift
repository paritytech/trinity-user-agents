#if DISABLE_AUTH
    import Foundation
    import UniqueDevice

    /// Null-object JWT manager used when the app is built with `-DDISABLE_AUTH`.
    ///
    /// Emits no bearer token and performs no attestation/token network calls, so
    /// requests go out unauthenticated. `BearerTokenRequestModifier` skips the
    /// `Authorization` header for the empty token this returns.
    final class NoAuthJWTTokenManager: JWTTokenProviding, JWTTokenManaging {
        func validToken() async throws -> String {
            ""
        }

        func invalidateToken() {}

        func withAuthorizedToken<R>(
            _ operation: @escaping (String) async throws -> R
        ) async throws -> R {
            try await operation("")
        }

        func setup(authProvider _: AppAttestProviding) {}

        func prewarm() {}
    }
#endif
