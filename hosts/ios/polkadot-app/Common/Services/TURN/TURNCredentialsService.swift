import Foundation
import UniqueDevice

protocol TURNCredentialsProviding: Sendable {
    func issueCredentials() async throws -> TURNCredentials
}

actor TURNCredentialsService: TURNCredentialsProviding {
    static let expiryBuffer: TimeInterval = 5

    private let requestFactory: TURNCredentialsRequestMaking
    private let tokenProvider: JWTTokenProviding
    private var store: [TURNIssueRequest: CachedCredentials] = [:]
    private var inflight: [TURNIssueRequest: Task<TURNCredentials, Error>] = [:]

    init(
        requestFactory: TURNCredentialsRequestMaking,
        tokenProvider: JWTTokenProviding
    ) {
        self.requestFactory = requestFactory
        self.tokenProvider = tokenProvider
    }

    func issueCredentials() async throws -> TURNCredentials {
        let request = TURNIssueRequest(regionHint: nil)

        if let cached = store[request], !isExpired(cached) {
            return cached.credentials
        }
        if let existing = inflight[request] {
            return try await existing.value
        }

        let task = Task { [requestFactory, tokenProvider] in
            try await tokenProvider.withAuthorizedToken { token in
                let modifier = BearerTokenRequestModifier(token: token)
                return try await requestFactory.issueCredentials(request: request, modifier: modifier)
            }
        }
        inflight[request] = task

        do {
            let credentials = try await task.value
            let expiresAt = Date().addingTimeInterval(TimeInterval(credentials.ttl))
            store[request] = CachedCredentials(credentials: credentials, expiresAt: expiresAt)
            inflight[request] = nil
            return credentials
        } catch {
            inflight[request] = nil
            throw error
        }
    }
}

private extension TURNCredentialsService {
    struct CachedCredentials {
        let credentials: TURNCredentials
        let expiresAt: Date
    }

    func isExpired(_ cached: CachedCredentials) -> Bool {
        Date().addingTimeInterval(Self.expiryBuffer) >= cached.expiresAt
    }
}
