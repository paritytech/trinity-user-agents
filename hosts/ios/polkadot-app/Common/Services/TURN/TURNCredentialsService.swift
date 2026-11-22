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
        try Task.checkCancellation()
        let request = TURNIssueRequest(regionHint: nil)

        if let cached = store[request], !isExpired(cached) {
            return cached.credentials
        }
        if let existing = inflight[request] {
            return try await credentials(from: existing)
        }

        let task = Task { [requestFactory, tokenProvider] in
            try Task.checkCancellation()
            return try await tokenProvider.withAuthorizedToken { token in
                try Task.checkCancellation()
                let modifier = BearerTokenRequestModifier(token: token)
                return try await requestFactory.issueCredentials(request: request, modifier: modifier)
            }
        }
        inflight[request] = task

        do {
            let credentials = try await self.credentials(from: task)
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
    func credentials(from task: Task<TURNCredentials, Error>) async throws -> TURNCredentials {
        try await withTaskCancellationHandler {
            let credentials = try await task.value
            try Task.checkCancellation()
            return credentials
        } onCancel: {
            // Task.value alone does not propagate cancellation to URLSession's request.
            task.cancel()
        }
    }

    struct CachedCredentials {
        let credentials: TURNCredentials
        let expiresAt: Date
    }

    func isExpired(_ cached: CachedCredentials) -> Bool {
        Date().addingTimeInterval(Self.expiryBuffer) >= cached.expiresAt
    }
}
