import Foundation
import Testing
import KeyDerivation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
import UniqueDevice
@testable import polkadot_app

@Suite("JWTTokenManager")
struct JWTTokenManagerTests {
    init() {
        AppConfigProvider.shared.apply(
            RemoteAppConfig(
                identityBackendUrl: URL(string: "https://polkadot-app-stg.parity.io/"),
                ipfsGatewayUrl: nil,
                dotNsResolver: nil,
                dotNsNameRegistry: nil,
                coinageInstanceId: nil,
                fundingUrl: nil,
                offrampUrl: nil,
                accountDataStoreContract: nil,
                paymentAsset: nil,
                appSharingUrl: nil
            )
        )
    }

    // MARK: - Cached token

    @Test("Returns cached token when available and not expired")
    func returnsCachedToken() async throws {
        let store = MockJWTTokenStore()
        try store.saveToken(validJWT)

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: StubHTTPLoader()
        )
        manager.setup(authProvider: MockAppAttestProvider())

        let token = try await manager.validToken()
        #expect(token == validJWT)
    }

    @Test("Does not fetch when cached token exists and not expired")
    func doesNotFetchWhenCached() async throws {
        let store = MockJWTTokenStore()
        try store.saveToken(validJWT)

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: StubHTTPLoader()
        )
        manager.setup(authProvider: MockAppAttestProvider())

        let token = try await manager.validToken()
        #expect(token == validJWT)
        // saveCallCount stays at 1 (initial save), no network fetch triggered
        #expect(store.saveCallCount == 1)
    }

    // MARK: - Invalidation

    @Test("Invalidate clears both access and refresh tokens")
    func invalidateClearsBothTokens() throws {
        let store = MockJWTTokenStore()
        try store.saveToken("access-token")
        try store.saveRefreshToken("refresh-token")

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: StubHTTPLoader()
        )
        manager.setup(authProvider: MockAppAttestProvider())

        manager.invalidateToken()
        #expect(store.fetchToken() == nil)
        #expect(store.fetchRefreshToken() == nil)
    }

    // MARK: - 401 retry

    @Test("withAuthorizedToken obtains fresh token via refresh on 401 and retries")
    func retriesOn401() async throws {
        let store = MockJWTTokenStore()
        try store.saveToken(validJWT)

        let loader = StubHTTPLoader()
        loader.setStub(
            .init(body: tokenResponseJSON(token: "fresh-access", refreshToken: "fresh-refresh")),
            for: AuthApi.token.url
        )

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader
        )
        manager.setup(authProvider: MockAppAttestProvider())

        var callCount = 0

        // First call throws 401, manager clears access token and re-obtains,
        // second call succeeds with the fresh token
        let result = try await manager.withAuthorizedToken { token in
            callCount += 1
            if callCount == 1 {
                throw BackendApiError(statusCode: .unauthorize, details: nil)
            }
            return token
        }

        #expect(callCount == 2)
        #expect(!result.isEmpty)
    }

    @Test("On 401, preserves a concurrently-refreshed token instead of clobbering it")
    func on401PreservesConcurrentlyRefreshedToken() async throws {
        let store = MockJWTTokenStore()
        let originalToken = makeJWT(exp: Date().timeIntervalSince1970 + 86_400)
        let concurrentlyRefreshed = makeJWT(exp: Date().timeIntervalSince1970 + 86_400 + 1)
        try store.saveToken(originalToken)

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: StubHTTPLoader()
        )
        manager.setup(authProvider: MockAppAttestProvider())

        var receivedTokens: [String] = []

        _ = try await manager.withAuthorizedToken { token in
            receivedTokens.append(token)
            if receivedTokens.count == 1 {
                // Simulate a sibling task refreshing the store before we get
                // our 401 back.
                try? store.saveToken(concurrentlyRefreshed)
                throw BackendApiError(statusCode: .unauthorize, details: nil)
            }
            return token
        }

        #expect(receivedTokens.count == 2)
        #expect(receivedTokens[0] == originalToken)
        #expect(receivedTokens[1] == concurrentlyRefreshed)
        #expect(store.fetchToken() == concurrentlyRefreshed)
        #expect(store.deleteCallCount == 0)
    }

    @Test("withAuthorizedToken passes through non-401 errors without clearing tokens")
    func passesThroughOtherErrors() async {
        let store = MockJWTTokenStore()
        try? store.saveToken(validJWT)
        try? store.saveRefreshToken("refresh-token")

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: StubHTTPLoader()
        )
        manager.setup(authProvider: MockAppAttestProvider())

        await #expect(throws: BackendApiError.self) {
            try await manager.withAuthorizedToken { _ in
                throw BackendApiError(statusCode: .internalServerError, details: "server down")
            }
        }

        // Non-401 errors should not clear any tokens
        #expect(store.fetchToken() == validJWT)
        #expect(store.fetchRefreshToken() == "refresh-token")
    }

    // MARK: - Token fetch saves both tokens

    @Test("Fetching token via attestation saves both access and refresh tokens")
    func fetchSavesBothTokens() async throws {
        let store = MockJWTTokenStore()

        let loader = StubHTTPLoader()
        loader.setStub(
            .init(body: tokenResponseJSON(token: "new-access", refreshToken: "new-refresh")),
            for: AuthApi.token.url
        )

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader
        )
        manager.setup(authProvider: MockAppAttestProvider())

        let token = try await manager.validToken()

        #expect(!token.isEmpty)
        #expect(store.fetchToken() != nil)
        #expect(store.fetchRefreshToken() != nil)
    }

    // MARK: - Refresh token flow

    @Test("On 401, second attempt uses a different token than the first one")
    func on401UsesNewToken() async throws {
        let store = MockJWTTokenStore()
        // Use a valid JWT so it passes expiry check and is returned to the operation
        let soonToBeRejected = validJWT
        try store.saveToken(soonToBeRejected)
        try store.saveRefreshToken("valid-refresh")

        let loader = StubHTTPLoader()
        loader.setStub(
            .init(body: tokenResponseJSON(token: "rotated-access", refreshToken: "rotated-refresh")),
            for: AuthApi.refreshToken.url
        )

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader
        )
        manager.setup(authProvider: MockAppAttestProvider())

        var receivedTokens: [String] = []

        _ = try await manager.withAuthorizedToken { token in
            receivedTokens.append(token)
            if receivedTokens.count == 1 {
                throw BackendApiError(statusCode: .unauthorize, details: nil)
            }
            return token
        }

        #expect(receivedTokens.count == 2)
        #expect(receivedTokens[0] == soonToBeRejected)
        #expect(receivedTokens[1] != soonToBeRejected)
    }

    // MARK: - Refresh fallback

    @Test("Uses refresh token when access token is cleared")
    func usesRefreshAfterAccessCleared() async throws {
        let store = MockJWTTokenStore()

        let loader = StubHTTPLoader()
        loader.setStub(
            .init(body: tokenResponseJSON(token: "access-1", refreshToken: "refresh-1")),
            for: AuthApi.token.url
        )
        loader.setStub(
            .init(body: tokenResponseJSON(token: "access-2", refreshToken: "refresh-2")),
            for: AuthApi.refreshToken.url
        )

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader
        )
        manager.setup(authProvider: MockAppAttestProvider())

        // First: get a real token pair via attestation
        let initialToken = try await manager.validToken()
        let initialRefresh = store.fetchRefreshToken()
        #expect(!initialToken.isEmpty)
        #expect(initialRefresh != nil)

        // Clear only access token (simulates debug menu action)
        store.deleteToken()
        #expect(store.fetchToken() == nil)
        #expect(store.fetchRefreshToken() != nil)

        // Second call tries refresh then falls back to attestation
        let refreshedToken = try await manager.validToken()

        #expect(!refreshedToken.isEmpty)
        // Refresh token must be rotated (single-use)
        let newRefresh = store.fetchRefreshToken()
        #expect(newRefresh != nil)
        #expect(newRefresh != initialRefresh)
    }

    @Test("Falls back to attestation when refresh token is invalid")
    func fallsBackWhenRefreshInvalid() async throws {
        let store = MockJWTTokenStore()
        // Fake refresh token that backend will reject
        try store.saveRefreshToken("invalid-refresh-token")

        // Refresh endpoint rejects, attestation succeeds.
        let loader = StubHTTPLoader()
        loader.setStub(
            .init(statusCode: 401, body: Data(#"{"error":"invalid"}"#.utf8)),
            for: AuthApi.refreshToken.url
        )
        loader.setStub(
            .init(body: tokenResponseJSON(token: "attested", refreshToken: "attested-refresh")),
            for: AuthApi.token.url
        )

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader
        )
        manager.setup(authProvider: MockAppAttestProvider())

        // Tries refresh (fails), falls back to attestation
        let token = try await manager.validToken()
        #expect(!token.isEmpty)
        #expect(store.fetchToken() != nil)
    }

    // MARK: - Proactive expiry

    @Test("Expired cached token triggers a fresh fetch")
    func expiredTokenTriggersRefresh() async throws {
        let store = MockJWTTokenStore()
        try store.saveToken(expiredJWT)

        let loader = StubHTTPLoader()
        loader.setStub(
            .init(body: tokenResponseJSON(token: validJWT, refreshToken: "fresh-refresh")),
            for: AuthApi.token.url
        )

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader
        )
        manager.setup(authProvider: MockAppAttestProvider())

        let token = try await manager.validToken()

        // Should have fetched a new token, not returned the expired one
        #expect(token != expiredJWT)
        #expect(!token.isEmpty)
    }

    @Test("Token expiring within buffer window triggers a fresh fetch")
    func almostExpiredTokenTriggersRefresh() async throws {
        let store = MockJWTTokenStore()
        try store.saveToken(almostExpiredJWT)

        let loader = StubHTTPLoader()
        loader.setStub(
            .init(body: tokenResponseJSON(token: validJWT, refreshToken: "fresh-refresh")),
            for: AuthApi.token.url
        )

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader
        )
        manager.setup(authProvider: MockAppAttestProvider())

        let token = try await manager.validToken()

        // almostExpiredJWT expires in 10s, buffer is 30s → treated as expired
        #expect(token != almostExpiredJWT)
        #expect(!token.isEmpty)
    }

    @Test("Token with plenty of time left is returned from cache")
    func validTokenReturnedFromCache() async throws {
        let store = MockJWTTokenStore()
        try store.saveToken(validJWT)

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: StubHTTPLoader()
        )
        manager.setup(authProvider: MockAppAttestProvider())

        let token = try await manager.validToken()
        #expect(token == validJWT)
        // Only the initial saveToken call, no fetch
        #expect(store.saveCallCount == 1)
    }

    @Test("dateProvider allows controlling time in tests")
    func dateProviderControlsExpiry() async throws {
        let futureJWT = makeJWT(exp: 1_000_000)
        let store = MockJWTTokenStore()
        try store.saveToken(futureJWT)

        let loader = StubHTTPLoader()
        loader.setStub(
            .init(body: tokenResponseJSON(token: "post-expiry", refreshToken: "post-refresh")),
            for: AuthApi.token.url
        )

        // Time is well before expiry → cached
        let managerBefore = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader,
            dateProvider: { Date(timeIntervalSince1970: 999_000) }
        )
        managerBefore.setup(authProvider: MockAppAttestProvider())
        let token1 = try await managerBefore.validToken()
        #expect(token1 == futureJWT)

        // Time is past expiry → fetch new
        let managerAfter = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader,
            dateProvider: { Date(timeIntervalSince1970: 1_000_001) }
        )
        managerAfter.setup(authProvider: MockAppAttestProvider())
        let token2 = try await managerAfter.validToken()
        #expect(token2 != futureJWT)
    }

    // MARK: - Coalescing

    @Test("Concurrent validToken calls share a single fetch")
    func concurrentCallsCoalesce() async throws {
        let store = MockJWTTokenStore()

        // Tiny delay so concurrent callers race into the same in-flight fetch.
        let loader = StubHTTPLoader()
        loader.setStub(
            .init(
                body: tokenResponseJSON(token: "shared-token", refreshToken: "shared-refresh"),
                delay: 0.05
            ),
            for: AuthApi.token.url
        )

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(),
            urlSession: loader
        )
        manager.setup(authProvider: MockAppAttestProvider())

        // Launch multiple concurrent token requests
        async let token1 = manager.validToken()
        async let token2 = manager.validToken()
        async let token3 = manager.validToken()

        let results = try await [token1, token2, token3]

        // All callers should get the same token
        #expect(results[0] == results[1])
        #expect(results[1] == results[2])
    }

    // MARK: - Backend-auth proof binding (Layer 2 sr25519)

    @Test("Token fetch invokes signer with proof bytes equal to SHA256(challenge ‖ pubkey ‖ SHA256({}))")
    func tokenFetchSignsExpectedProof() async throws {
        let store = MockJWTTokenStore()
        let wallet = MockBackendAuthWallet()

        let loader = StubHTTPLoader()
        loader.setStub(
            .init(body: tokenResponseJSON(token: "fresh-access", refreshToken: "fresh-refresh")),
            for: AuthApi.token.url
        )

        let manager = JWTTokenManager(
            tokenStore: store,
            authStore: StubBackendAuthStore(wallet: wallet),
            urlSession: loader
        )
        manager.setup(authProvider: MockAppAttestProvider())

        _ = try await manager.validToken()

        #expect(wallet.signCallCount == 1)

        let body = Data("{}".utf8)
        let expectedProof = (testChallenge + wallet.pubkey + body.sha256()).sha256()
        #expect(wallet.capturedSignedData == expectedProof)
    }
}

// MARK: - Helpers

private func tokenResponseJSON(token: String, refreshToken: String) -> Data {
    let json = #"{"token":"\#(token)","refreshToken":"\#(refreshToken)"}"#
    return Data(json.utf8)
}

// MARK: - JWT Helpers

/// Builds a minimal JWT string with the given `exp` timestamp.
/// The signature is fake — only the payload matters for expiry checks.
private func makeJWT(exp: TimeInterval) -> String {
    let header = Data(#"{"alg":"HS256"}"#.utf8).base64EncodedString()
    let payload = Data(#"{"sub":"test","exp":\#(Int(exp))}"#.utf8).base64EncodedString()
    return "\(header).\(payload).fake-signature"
}

/// A JWT that expires far in the future (valid).
private let validJWT = makeJWT(exp: Date().timeIntervalSince1970 + 86_400)

/// A JWT that expired in the past.
private let expiredJWT = makeJWT(exp: Date().timeIntervalSince1970 - 60)

/// A JWT that expires within the 30s buffer window.
private let almostExpiredJWT = makeJWT(exp: Date().timeIntervalSince1970 + 10)

// MARK: - Mock AppAttestProvider for tests

/// Fixed challenge surfaced by the mock provider — emitted as base64 in
/// `Auth-Challenge`, the way `AppAttestRequestModifier` would in production.
/// `BackendAuthProofRequestModifier` reads it back to compute the sr25519 proof.
private let testChallenge = Data(repeating: 0x55, count: 32)

private final class MockAppAttestProvider: AppAttestProviding {
    func setup() {}

    func appAttestModifier(
        for bodyDataClosure: @escaping () throws -> Data?,
        clientIdClosure: (() throws -> Data)?
    ) -> CompoundOperationWrapper<any HttpRequestModifier> {
        let operation: BaseOperation<any HttpRequestModifier> = ClosureOperation {
            try MockRequestModifier(
                data: bodyDataClosure(),
                clientId: clientIdClosure?(),
                challenge: testChallenge
            )
        }
        return CompoundOperationWrapper(targetOperation: operation)
    }
}

private final class MockRequestModifier: HttpRequestModifier {
    let data: Data?
    let clientId: Data?
    let challenge: Data

    init(data: Data?, clientId: Data?, challenge: Data) {
        self.data = data
        self.clientId = clientId
        self.challenge = challenge
    }

    func visit(request: inout URLRequest) throws {
        request.httpBody = data
        request.setValue(challenge.base64EncodedString(), forHTTPHeaderField: "Auth-Challenge")

        if let clientId {
            request.setValue(clientId.base64EncodedString(), forHTTPHeaderField: "Auth-ClientId")
        }
    }
}

// MARK: - Stub backend auth store for tests

private final class StubBackendAuthStore: BackendAuthStoring {
    let wallet: WalletManaging

    init(wallet: WalletManaging = MockBackendAuthWallet()) {
        self.wallet = wallet
    }

    func fetchAuthWallet() throws -> WalletManaging { wallet }
}

// MARK: - Mock backend-auth wallet for tests

private final class MockBackendAuthWallet: WalletManaging, @unchecked Sendable {
    let pubkey: Data
    let privkey: Data
    private let lock = NSLock()
    private(set) var capturedSignedData: Data?
    private(set) var signCallCount = 0

    init(
        pubkey: Data = Data(repeating: 0xAB, count: 32),
        privkey: Data = Data(repeating: 0xBB, count: 32)
    ) {
        self.pubkey = pubkey
        self.privkey = privkey
    }

    func getRawPublicKey() throws -> Data { pubkey }

    func fetchRawSecretKey() throws -> Data { privkey }

    func sign(data: Data) throws -> MultiSignature {
        lock.withLock {
            signCallCount += 1
            capturedSignedData = data
        }
        return .sr25519(data: Data(repeating: 0xCD, count: 64))
    }

    func getMultiSigner() throws -> MultiSigner { .sr25519(pubkey) }

    func fetchSignerSecret(for _: SignerProviding) throws -> Data {
        fatalError("not used in tests")
    }

    func fetchAccount(for _: ChainProtocol) throws -> AccountProtocol {
        fatalError("not used in tests")
    }

    func hasAccount(in _: ChainProtocol) -> Bool { true }
}
