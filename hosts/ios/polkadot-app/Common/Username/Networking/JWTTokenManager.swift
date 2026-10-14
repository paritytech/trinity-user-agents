import Foundation
import FoundationExt
import KeyDerivation
import Operation_iOS
import StructuredConcurrency
import UniqueDevice
import os

protocol JWTTokenProviding {
    func validToken() async throws -> String
    func invalidateToken()

    /// Executes an async closure with a valid token, retrying once on 401.
    func withAuthorizedToken<R>(
        _ operation: @escaping (String) async throws -> R
    ) async throws -> R
}

protocol JWTTokenManaging {
    func setup(authProvider: AppAttestProviding)
    func prewarm()
}

enum JWTTokenManagerError: Error {
    case noAuthProvider
}

final class JWTTokenManager: JWTTokenProviding, JWTTokenManaging {
    /// Buffer before actual expiry to proactively refresh the token.
    static let expiryBuffer: TimeInterval = 30

    #if DISABLE_AUTH
        static let shared: JWTTokenProviding & JWTTokenManaging = NoAuthJWTTokenManager()
    #else
        static let shared = JWTTokenManager()
    #endif

    private let tokenStore: JWTTokenStoring
    private var authProvider: AppAttestProviding?
    private let authStore: BackendAuthStoring
    private let urlSession: any HTTPDataLoading
    private let storeLock = OSAllocatedUnfairLock()
    private let tokenFetch = CoalescingTask<String>()
    private let dateProvider: () -> Date
    private let logger: LoggerProtocol

    init(
        tokenStore: JWTTokenStoring = JWTTokenStore(),
        authStore: BackendAuthStoring = BackendAuthStore(),
        urlSession: any HTTPDataLoading = URLSession.shared,
        dateProvider: @escaping () -> Date = { Date() },
        logger: LoggerProtocol = Logger.shared
    ) {
        self.tokenStore = tokenStore
        self.authStore = authStore
        self.urlSession = urlSession
        self.dateProvider = dateProvider
        self.logger = logger
    }

    func setup(authProvider: AppAttestProviding) {
        self.authProvider = authProvider
    }

    func prewarm() {
        Task {
            _ = try? await validToken()
        }
    }

    // MARK: - async/await API

    func validToken() async throws -> String {
        if let cached = storeLock.withLock({ tokenStore.fetchToken() }),
           !isTokenExpired(cached) {
            logger.debug("Returning cached token")
            return cached
        }

        logger.debug("Requesting new token...")

        do {
            let token = try await tokenFetch.run { try await self.obtainToken() }
            logger.debug("Did receive new token")
            return token
        } catch {
            logger.error("Token fetch failed: \(error)")
            throw error
        }
    }

    func invalidateToken() {
        storeLock.withLock { tokenStore.deleteAll() }
    }

    func withAuthorizedToken<R>(
        _ operation: @escaping (String) async throws -> R
    ) async throws -> R {
        let token = try await validToken()

        do {
            return try await operation(token)
        } catch let error as BackendApiError where error.statusCode == .unauthorize {
            // Only evict the token we used; preserve any concurrently-refreshed token.
            storeLock.withLock {
                if tokenStore.fetchToken() == token {
                    tokenStore.deleteToken()
                }
            }

            let freshToken = try await validToken()
            return try await operation(freshToken)
        }
    }
}

// MARK: - Private

private extension JWTTokenManager {
    /// Treats any parse failure or missing `exp` claim as expired (safe fallback).
    func isTokenExpired(_ token: String) -> Bool {
        guard let payload = try? JWTParser.parse(token), let exp = payload.exp else {
            return true
        }
        return dateProvider().addingTimeInterval(Self.expiryBuffer) >= exp
    }

    /// Tries refresh token first, falls back to full attestation.
    func obtainToken() async throws -> String {
        if let refreshToken = storeLock.withLock({ tokenStore.fetchRefreshToken() }) {
            do {
                return try await refreshAccessToken(using: refreshToken)
            } catch {
                storeLock.withLock { tokenStore.deleteAll() }
            }
        }

        return try await fetchTokenViaAttestation()
    }

    /// Full attestation flow: challenge → attestation → /auth/token
    func fetchTokenViaAttestation() async throws -> String {
        guard let authProvider else {
            throw JWTTokenManagerError.noAuthProvider
        }

        logger.debug("Getting token via attestation")

        let wallet = try authStore.fetchAuthWallet()
        let clientId = try wallet.getRawPublicKey()

        let appAttestModifier = try await authProvider.appAttestModifier(
            for: { Data("{}".utf8) },
            clientIdClosure: { clientId }
        )
        .asyncExecute()

        logger.debug("Received attestation")

        let modifier = BackendAuthProofRequestModifier(
            inner: appAttestModifier,
            wallet: wallet
        )

        let response: JWTTokenResponse = try await sendJSONPost(
            url: AuthApi.token.url,
            body: Data("{}".utf8),
            modifier: modifier
        )

        logger.debug("Received token")

        persist(response: response)
        return response.token
    }

    /// Exchange refresh token for new token pair via /auth/token/refresh
    func refreshAccessToken(using refreshToken: String) async throws -> String {
        logger.debug("Getting token via refresh access token")

        let body = try JSONEncoder().encode(RefreshTokenRequest(refreshToken: refreshToken))
        let response: JWTTokenResponse = try await sendJSONPost(
            url: AuthApi.refreshToken.url,
            body: body
        )

        logger.debug("Received token")

        persist(response: response)
        return response.token
    }

    func persist(response: JWTTokenResponse) {
        storeLock.withLock {
            do {
                try tokenStore.saveToken(response.token)
                try tokenStore.saveRefreshToken(response.refreshToken)
            } catch {
                Logger.shared.error("Failed to save JWT tokens to keychain: \(error)")
            }
        }
    }

    /// Throws `BackendApiError` on non-OK status so 401 retry in `withAuthorizedToken` fires.
    func sendJSONPost<Response: Decodable>(
        url: URL,
        body: Data,
        modifier: (any HttpRequestModifier)? = nil
    ) async throws -> Response {
        var request = URLRequest(url: url)
        request.httpMethod = HttpMethod.post.rawValue
        request.setValue(
            HttpContentType.json.rawValue,
            forHTTPHeaderField: HttpHeaderKey.contentType.rawValue
        )
        request.httpBody = body

        if let modifier {
            try modifier.visit(request: &request)
        }

        let (data, urlResponse) = try await urlSession.data(for: request)

        guard let httpResponse = urlResponse as? HTTPURLResponse else {
            throw NetworkBaseError.unexpectedResponseObject
        }

        guard let statusCode = BackendStatusCode(rawValue: httpResponse.statusCode) else {
            throw NetworkResponseError.unexpectedStatusCode
        }

        guard statusCode.isOk else {
            let details = String(data: data, encoding: .utf8)
            throw BackendApiError(statusCode: statusCode, details: details)
        }

        return try JSONDecoder().decode(Response.self, from: data)
    }
}
