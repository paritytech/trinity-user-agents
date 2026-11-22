import Foundation
import StructuredConcurrency
import UniqueDevice

protocol TURNCredentialsProviding: Sendable {
    func issueCredentials() async throws -> TURNCredentials
}

actor TURNCredentialsService: TURNCredentialsProviding {
    static let expiryBuffer: TimeInterval = 5

    private let requestFactory: TURNCredentialsRequestMaking
    private let tokenProvider: JWTTokenProviding
    private let clock: any Clock<Duration>
    private var store: [TURNIssueRequest: CachedCredentials] = [:]
    private var inflight: [TURNIssueRequest: PendingRequest] = [:]

    init(
        requestFactory: TURNCredentialsRequestMaking,
        tokenProvider: JWTTokenProviding,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.requestFactory = requestFactory
        self.tokenProvider = tokenProvider
        self.clock = clock
    }

    func issueCredentials() async throws -> TURNCredentials {
        try Task.checkCancellation()
        let request = TURNIssueRequest(regionHint: nil)

        if let cached = store[request], !isExpired(cached) {
            return cached.credentials
        }
        let waiter = UUID()
        let (stream, continuation) = AsyncThrowingStream<TURNCredentials, Error>.makeStream()
        if inflight[request] == nil {
            startRequest(request)
        }
        inflight[request]?.waiters[waiter] = continuation
        defer { removeWaiter(waiter, from: request) }

        var iterator = stream.makeAsyncIterator()
        let credentials = try await iterator.next()
        try Task.checkCancellation()
        guard let credentials else { throw CancellationError() }
        return credentials
    }
}

private extension TURNCredentialsService {
    func startRequest(_ request: TURNIssueRequest) {
        let id = UUID()
        let task = Task { [requestFactory, tokenProvider] in
            do {
                try Task.checkCancellation()
                let credentials = try await tokenProvider.withAuthorizedToken { token in
                    try Task.checkCancellation()
                    let modifier = BearerTokenRequestModifier(token: token)
                    return try await requestFactory.issueCredentials(request: request, modifier: modifier)
                }
                complete(request, id: id, result: .success(credentials))
            } catch {
                complete(request, id: id, result: .failure(error))
            }
        }
        let deadline = Task { [clock] in
            do {
                try await clock.sleep(for: .seconds(15))
                complete(request, id: id, result: .failure(TimeoutError()))
            } catch {
                // Completion or the last consumer's cancellation already ended the request.
            }
        }
        inflight[request] = PendingRequest(id: id, task: task, deadline: deadline, waiters: [:])
    }

    func complete(_ request: TURNIssueRequest, id: UUID, result: Result<TURNCredentials, Error>) {
        guard let pending = inflight[request], pending.id == id else { return }
        inflight[request] = nil
        pending.task.cancel()
        pending.deadline.cancel()
        if case let .success(credentials) = result {
            let expiresAt = Date().addingTimeInterval(TimeInterval(credentials.ttl))
            store[request] = CachedCredentials(credentials: credentials, expiresAt: expiresAt)
        }
        for continuation in pending.waiters.values {
            switch result {
            case let .success(credentials):
                continuation.yield(credentials)
                continuation.finish()
            case let .failure(error):
                continuation.finish(throwing: error)
            }
        }
    }

    func removeWaiter(_ waiter: UUID, from request: TURNIssueRequest) {
        guard inflight[request]?.waiters.removeValue(forKey: waiter) != nil else { return }
        if let pending = inflight[request], pending.waiters.isEmpty {
            inflight[request] = nil
            pending.task.cancel()
            pending.deadline.cancel()
        }
    }

    struct PendingRequest {
        let id: UUID
        let task: Task<Void, Never>
        let deadline: Task<Void, Never>
        var waiters: [UUID: AsyncThrowingStream<TURNCredentials, Error>.Continuation]
    }

    struct CachedCredentials {
        let credentials: TURNCredentials
        let expiresAt: Date
    }

    func isExpired(_ cached: CachedCredentials) -> Bool {
        Date().addingTimeInterval(Self.expiryBuffer) >= cached.expiresAt
    }
}
