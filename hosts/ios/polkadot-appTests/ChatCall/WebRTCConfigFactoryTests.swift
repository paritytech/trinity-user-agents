@testable import polkadot_app
import Clocks
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

    @Test("Media returns direct ICE at two seconds while TURN warms the cache at three seconds")
    func slowCredentialsWarmCache() async throws {
        let clock = TestClock<Duration>()
        let http = TimedHTTP(clock: clock, responseDelay: .seconds(3))
        let service = makeService(http: http, clock: clock)
        let factory = WebRTCConfigFactory(turnService: service, allowsDirectFallback: true, clock: clock)
        let firstPeer = Task { try await factory.makeConnectionConfiguration() }
        await http.waitUntilStarted()

        await clock.advance(by: .seconds(2))
        let direct = try await firstPeer.value
        #expect(direct.iceServers.isEmpty)
        #expect(await http.cancellationCount == 0)
        #expect(await http.requestCount == 1)

        await clock.advance(by: .seconds(1))
        let warmed = try await service.issueCredentials()
        let secondPeer = try await factory.makeConnectionConfiguration()
        #expect(secondPeer.iceServers.flatMap(\.urlStrings) == warmed.servers)
        #expect(warmed.servers == ["turn:relay.example:3478"])
        #expect(await http.cancellationCount == 0)
        #expect(await http.requestCount == 1)
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

    private actor TimedHTTP: HTTPDataLoading {
        let clock: TestClock<Duration>
        let responseDelay: Duration
        private let starts = AsyncStream<Void>.makeStream()
        private let cancellations = AsyncStream<Void>.makeStream()
        private(set) var requestCount = 0
        private(set) var cancellationCount = 0

        init(clock: TestClock<Duration>, responseDelay: Duration) {
            self.clock = clock
            self.responseDelay = responseDelay
        }

        func data(for request: URLRequest) async throws -> (Data, URLResponse) {
            requestCount += 1
            starts.continuation.yield(())
            do {
                try await clock.sleep(for: responseDelay)
            } catch {
                if Task.isCancelled {
                    cancellationCount += 1
                    cancellations.continuation.yield(())
                }
                throw error
            }
            let data = Data(#"{"servers":["turn:relay.example:3478"],"username":"test","password":"test","ttl":60}"#.utf8)
            let response = try #require(HTTPURLResponse(
                url: #require(request.url), statusCode: 201, httpVersion: nil, headerFields: nil
            ))
            return (data, response)
        }

        func waitUntilStarted() async {
            var iterator = starts.stream.makeAsyncIterator()
            await iterator.next()
        }

        func waitUntilCancelled() async {
            var iterator = cancellations.stream.makeAsyncIterator()
            await iterator.next()
        }
    }

    private func makeService(http: TimedHTTP, clock: TestClock<Duration>) -> TURNCredentialsService {
        TURNCredentialsService(
            requestFactory: TURNCredentialsRequestFactory(urlSession: http),
            tokenProvider: MockJWTTokenProvider(),
            clock: clock
        )
    }

    @Test("The shared TURN request has its own fifteen-second deadline")
    func credentialServiceBoundsNetwork() async throws {
        let clock = TestClock<Duration>()
        let http = TimedHTTP(clock: clock, responseDelay: .seconds(60))
        let service = makeService(http: http, clock: clock)
        let factory = WebRTCConfigFactory(turnService: service, allowsDirectFallback: true, clock: clock)
        let media = Task { try await factory.makeConnectionConfiguration() }
        await http.waitUntilStarted()
        await clock.advance(by: .seconds(2))
        let configuration = try await media.value
        #expect(configuration.iceServers.isEmpty)
        #expect(await http.cancellationCount == 0)

        await clock.advance(by: .seconds(12))
        #expect(await http.cancellationCount == 0)
        await clock.advance(by: .seconds(1))
        await http.waitUntilCancelled()
        #expect(await http.cancellationCount == 1)
        #expect(await http.requestCount == 1)
    }

    @Test("Cancelling Media preserves a concurrent Chat request")
    func mediaCancellationPreservesChat() async throws {
        let clock = TestClock<Duration>()
        let http = TimedHTTP(clock: clock, responseDelay: .seconds(3))
        let service = makeService(http: http, clock: clock)
        let mediaFactory = WebRTCConfigFactory(turnService: service, allowsDirectFallback: true, clock: clock)
        let chatFactory = WebRTCConfigFactory(turnService: service, clock: clock)
        let chat = Task { try await chatFactory.makeConnectionConfiguration() }
        await http.waitUntilStarted()
        let media = Task { try await mediaFactory.makeConnectionConfiguration() }
        await clock.advance(by: .seconds(1))
        media.cancel()
        await #expect(throws: CancellationError.self) { try await media.value }
        #expect(await http.cancellationCount == 0)

        await clock.advance(by: .seconds(2))
        let configuration = try await chat.value
        #expect(configuration.iceServers.flatMap(\.urlStrings) == ["turn:relay.example:3478"])
        #expect(await http.requestCount == 1)
        #expect(await http.cancellationCount == 0)
    }

    @Test("Cancelling Chat preserves Media and its cache warmup")
    func chatCancellationPreservesMedia() async throws {
        let clock = TestClock<Duration>()
        let http = TimedHTTP(clock: clock, responseDelay: .seconds(3))
        let service = makeService(http: http, clock: clock)
        let mediaFactory = WebRTCConfigFactory(turnService: service, allowsDirectFallback: true, clock: clock)
        let chatFactory = WebRTCConfigFactory(turnService: service, clock: clock)
        let media = Task { try await mediaFactory.makeConnectionConfiguration() }
        await http.waitUntilStarted()
        let chat = Task { try await chatFactory.makeConnectionConfiguration() }
        await clock.advance(by: .seconds(1))
        chat.cancel()
        await #expect(throws: CancellationError.self) { try await chat.value }
        #expect(await http.cancellationCount == 0)

        await clock.advance(by: .seconds(1))
        let direct = try await media.value
        #expect(direct.iceServers.isEmpty)
        await clock.advance(by: .seconds(1))
        let cached = try await mediaFactory.makeConnectionConfiguration()
        #expect(cached.iceServers.flatMap(\.urlStrings) == ["turn:relay.example:3478"])
        #expect(await http.requestCount == 1)
        #expect(await http.cancellationCount == 0)
    }

    @Test("Cancelling every consumer stops the shared network request", arguments: [false, true])
    func lastConsumerCancelsNetwork(hasConcurrentChat: Bool) async {
        let clock = TestClock<Duration>()
        let http = TimedHTTP(clock: clock, responseDelay: .seconds(60))
        let service = makeService(http: http, clock: clock)
        let factory = WebRTCConfigFactory(turnService: service, allowsDirectFallback: true, clock: clock)
        let media = Task { try await factory.makeConnectionConfiguration() }
        await http.waitUntilStarted()
        let chat: Task<TURNCredentials, Error>? = hasConcurrentChat
            ? Task { try await service.issueCredentials() }
            : nil
        await clock.advance(by: .seconds(1))
        media.cancel()
        await #expect(throws: CancellationError.self) { try await media.value }
        if let chat {
            #expect(await http.cancellationCount == 0)
            chat.cancel()
            await #expect(throws: CancellationError.self) { try await chat.value }
        }
        await http.waitUntilCancelled()
        #expect(await http.cancellationCount == 1)
        #expect(await http.requestCount == 1)
    }
}
