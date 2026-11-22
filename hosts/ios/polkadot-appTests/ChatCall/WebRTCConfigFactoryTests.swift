@testable import polkadot_app
import Foundation
import Testing
import WebRTC

@Suite("WebRTC optional TURN configuration", .timeLimit(.minutes(1)))
struct WebRTCConfigFactoryTests {
    private enum Failure: Error { case unavailable }

    private struct CredentialsProvider: TURNCredentialsProviding {
        let result: Result<TURNCredentials, Error>

        func issueCredentials() async throws -> TURNCredentials {
            try result.get()
        }
    }

    @Test("Media retains direct ICE when TURN acquisition fails")
    func directFallback() async throws {
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .failure(Failure.unavailable)),
            allowsDirectFallback: true
        )
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceServers.isEmpty)
        #expect(configuration.iceTransportPolicy == .all)
        #expect(configuration.sdpSemantics == .unifiedPlan)
        #expect(configuration.bundlePolicy == .maxBundle)
        #expect(configuration.rtcpMuxPolicy == .require)
        #expect(configuration.iceCandidatePoolSize == 8)
    }

    @Test("Existing Chat callers retain TURN failure behavior")
    func defaultStillThrows() async {
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .failure(Failure.unavailable))
        )
        await #expect(throws: Failure.self) {
            try await factory.makeConnectionConfiguration()
        }
    }

    @Test("Provider-local cancellation leaves Media direct ICE available")
    func providerCancellationFallsBack() async throws {
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .failure(CancellationError())),
            allowsDirectFallback: true
        )
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceServers.isEmpty)
    }

    @Test("Provider-local network cancellation leaves Media direct ICE available")
    func cancelledRequestFallsBack() async throws {
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .failure(URLError(.cancelled))),
            allowsDirectFallback: true
        )
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceServers.isEmpty)
    }

    @Test("TURN servers supplement rather than replace direct ICE", arguments: [[], ["turn:relay.example:3478"]])
    func availableCredentials(servers: [String]) async throws {
        let credentials = TURNCredentials(servers: servers, username: "test", password: "test", ttl: 60)
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .success(credentials)),
            allowsDirectFallback: true
        )
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceServers.flatMap(\.urlStrings) == servers)
        #expect(configuration.iceTransportPolicy == .all)
    }

    private struct SuspendedProvider: TURNCredentialsProviding {
        let started: AsyncStream<Void>.Continuation
        let cancelled: AsyncStream<Void>.Continuation

        func issueCredentials() async throws -> TURNCredentials {
            try await withTaskCancellationHandler {
                started.yield(())
                try await Task.sleep(for: .seconds(60))
                throw Failure.unavailable
            } onCancel: {
                cancelled.yield(())
            }
        }
    }

    @Test("Media bounds a suspended provider and cancels its request")
    func suspendedProviderTimesOut() async throws {
        let (started, start) = AsyncStream<Void>.makeStream()
        let (cancelled, cancel) = AsyncStream<Void>.makeStream()
        let factory = WebRTCConfigFactory(
            turnService: SuspendedProvider(started: start, cancelled: cancel),
            allowsDirectFallback: true
        )
        let began = ContinuousClock.now
        let task = Task { try await factory.makeConnectionConfiguration() }
        var starts = started.makeAsyncIterator()
        await starts.next()
        let configuration = try await task.value
        #expect(configuration.iceServers.isEmpty)
        #expect(began.duration(to: .now) < .seconds(4))
        var cancellations = cancelled.makeAsyncIterator()
        await cancellations.next()
    }

    @Test("Outer cancellation stops the provider instead of returning direct ICE")
    func outerCancellationIsNotFallback() async {
        let (started, start) = AsyncStream<Void>.makeStream()
        let (cancelled, cancel) = AsyncStream<Void>.makeStream()
        let factory = WebRTCConfigFactory(
            turnService: SuspendedProvider(started: start, cancelled: cancel),
            allowsDirectFallback: true
        )
        let task = Task { try await factory.makeConnectionConfiguration() }
        var starts = started.makeAsyncIterator()
        await starts.next()
        task.cancel()
        await #expect(throws: CancellationError.self) { try await task.value }
        var cancellations = cancelled.makeAsyncIterator()
        await cancellations.next()
    }

    @Test("Already-cancelled configuration cannot return direct ICE")
    func cancelledBeforeConfiguration() async {
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .success(
                TURNCredentials(servers: [], username: "", password: "", ttl: 60)
            )),
            allowsDirectFallback: true
        )
        let task = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            return try await factory.makeConnectionConfiguration()
        }
        await #expect(throws: CancellationError.self) { try await task.value }
    }

    private final class UncooperativeProvider: TURNCredentialsProviding, @unchecked Sendable {
        private let lock = NSLock()
        private var continuation: CheckedContinuation<TURNCredentials, Error>?

        func issueCredentials() async throws -> TURNCredentials {
            try await withCheckedThrowingContinuation { continuation in
                lock.withLock { self.continuation = continuation }
            }
        }

        func release() {
            let pending = lock.withLock {
                let pending = continuation
                continuation = nil
                return pending
            }
            pending?.resume(throwing: Failure.unavailable)
        }
    }

    @Test("Media never waits for a cancellation-ignoring provider to finish")
    func uncooperativeProviderCannotHoldDeadline() async throws {
        let provider = UncooperativeProvider()
        let factory = WebRTCConfigFactory(turnService: provider, allowsDirectFallback: true)
        // A regressed task-group timeout must fail this test, not strand its runner.
        let cleanup = Task {
            try await Task.sleep(for: .seconds(4))
            provider.release()
        }
        defer { cleanup.cancel(); provider.release() }
        let began = ContinuousClock.now
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceServers.isEmpty)
        #expect(began.duration(to: .now) < .seconds(3))
    }

    private struct SuspendedHTTP: HTTPDataLoading {
        let provider: SuspendedProvider

        func data(for request: URLRequest) async throws -> (Data, URLResponse) {
            _ = try await provider.issueCredentials()
            throw Failure.unavailable
        }
    }

    private struct MalformedHTTP: HTTPDataLoading {
        func data(for request: URLRequest) async throws -> (Data, URLResponse) {
            let response = try #require(HTTPURLResponse(
                url: try #require(request.url), statusCode: 200, httpVersion: nil, headerFields: nil
            ))
            return (Data(#"{"servers":false}"#.utf8), response)
        }
    }

    @Test("Malformed TURN responses leave Media direct ICE available")
    func malformedCredentialsFallBack() async throws {
        let service = TURNCredentialsService(
            requestFactory: TURNCredentialsRequestFactory(urlSession: MalformedHTTP()),
            tokenProvider: MockJWTTokenProvider()
        )
        let factory = WebRTCConfigFactory(turnService: service, allowsDirectFallback: true)
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceServers.isEmpty)
        #expect(configuration.iceTransportPolicy == .all)
    }

    @Test("Media timeout reaches the TURN service's underlying HTTP request")
    func credentialServiceCancelsNetwork() async throws {
        let (started, start) = AsyncStream<Void>.makeStream()
        let (cancelled, cancel) = AsyncStream<Void>.makeStream()
        let service = TURNCredentialsService(
            requestFactory: TURNCredentialsRequestFactory(urlSession: SuspendedHTTP(
                provider: SuspendedProvider(started: start, cancelled: cancel)
            )),
            tokenProvider: MockJWTTokenProvider()
        )
        let factory = WebRTCConfigFactory(turnService: service, allowsDirectFallback: true)
        let task = Task { try await factory.makeConnectionConfiguration() }
        var starts = started.makeAsyncIterator()
        await starts.next()
        let configuration = try await task.value
        #expect(configuration.iceServers.isEmpty)
        var cancellations = cancelled.makeAsyncIterator()
        await cancellations.next()
    }
}
