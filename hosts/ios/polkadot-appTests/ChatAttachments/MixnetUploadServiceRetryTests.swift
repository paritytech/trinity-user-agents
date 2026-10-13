import Foundation
import HandoffService
import Individuality
import AsyncExtensions
import Operation_iOS
import StructuredConcurrency
import Testing
import SDKLogger
import SubstrateSdk

@testable import polkadot_app

// MARK: - Upload retry

extension MixnetUploadServiceTests {
    @Test("transport drop is retried and the upload completes")
    func retriesOnRemoteCancelled() async throws {
        let mockLoader = makeDropThenSucceedLoader()

        let loaderFactory = MockHOPFileLoaderFactory(loader: mockLoader)
        let env = makeTestEnv(loaderFactory: loaderFactory)
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(
            makeUploadContent(),
            status: .outgoing(.new)
        )
        env.service.setup()

        let event = await awaitTerminalEvent(
            for: makeAttachmentId(message: message),
            in: env.service
        )

        if case .onComplete = event {
            // expected
        } else {
            Issue.record("Expected onComplete after retry, got \(String(describing: event))")
        }

        #expect(mockLoader.uploadAttemptCount == 2)
        #expect(loaderFactory.lastRequestedNode == MockHopNodes.trusted)

        env.service.throttle()
    }

    @Test("no failure event is published between retry attempts")
    func noFailureEventBetweenAttempts() async throws {
        let mockLoader = makeDropThenSucceedLoader()

        let env = makeTestEnv(loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader))
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(
            makeUploadContent(),
            status: .outgoing(.new)
        )

        let attachmentId = makeAttachmentId(message: message)

        // Subscribe BEFORE the upload starts so no event can be missed.
        let stream = await env.service.subscribeState(for: attachmentId)
        let observer = TerminalEventObserver()

        let collector = Task {
            for try await event in stream {
                switch event {
                case .onFailure:
                    await observer.record(sawFailure: true)
                    return
                case .onComplete:
                    await observer.record(sawFailure: false)
                    return
                case .onProgress,
                     nil:
                    continue
                }
            }
        }

        env.service.setup()

        guard let sawFailure = await observer.awaitOutcome() else {
            Issue.record("No terminal event arrived before the deadline")
            collector.cancel()
            env.service.throttle()
            return
        }

        #expect(sawFailure == false)
        #expect(mockLoader.uploadAttemptCount == 2)

        collector.cancel()
        env.service.throttle()
    }

    @Test("failure is published once retries are exhausted")
    func failureAfterExhaustion() async throws {
        let mockLoader = MockHOPFileLoader()
        // Single script repeats for every attempt.
        mockLoader.uploadEventsPerAttempt = [
            [.onError(JSONRPCEngineError.remoteCancelled)]
        ]

        let policy = UploadRetryPolicy.fastRetries
        let env = makeTestEnv(
            loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader),
            retryPolicy: policy
        )
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(
            makeUploadContent(),
            status: .outgoing(.new)
        )
        env.service.setup()

        let event = await awaitTerminalEvent(
            for: makeAttachmentId(message: message),
            in: env.service
        )

        if case .onFailure = event {
            // expected
        } else {
            Issue.record("Expected onFailure after exhaustion, got \(String(describing: event))")
        }

        #expect(mockLoader.uploadAttemptCount == policy.maxAttempts)

        env.service.throttle()
    }

    @Test("non-transport error fails fast without retrying")
    func nonTransportErrorNotRetried() async throws {
        let mockLoader = MockHOPFileLoader()
        mockLoader.uploadEventsPerAttempt = [
            [.onError(NSError(domain: "test", code: 42))]
        ]

        let env = makeTestEnv(loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader))
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(
            makeUploadContent(),
            status: .outgoing(.new)
        )
        env.service.setup()

        let event = await awaitTerminalEvent(
            for: makeAttachmentId(message: message),
            in: env.service
        )

        if case .onFailure = event {
            // expected
        } else {
            Issue.record("Expected onFailure, got \(String(describing: event))")
        }

        #expect(mockLoader.uploadAttemptCount == 1)

        env.service.throttle()
    }

    @Test("pool-full is retried")
    func poolFullRetried() async throws {
        let mockLoader = MockHOPFileLoader()
        mockLoader.uploadEventsPerAttempt = [
            [.onError(JSONRPCError(message: "Pool full", code: HOPErrorCode.poolFull, data: nil))],
            [
                .onProgress(.init(uploaded: 500, total: 500, uploadedHashes: [Data()])),
                .onFinished(.chunked(metadata: Data(repeating: 0xFF, count: 32)))
            ]
        ]

        let env = makeTestEnv(loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader))
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(
            makeUploadContent(),
            status: .outgoing(.new)
        )
        env.service.setup()

        let event = await awaitTerminalEvent(
            for: makeAttachmentId(message: message),
            in: env.service
        )

        if case .onComplete = event {
            // expected
        } else {
            Issue.record("Expected onComplete after pool-full retry, got \(String(describing: event))")
        }

        #expect(mockLoader.uploadAttemptCount == 2)

        env.service.throttle()
    }

    @Test("rate-limited is retried")
    func rateLimitedRetried() async throws {
        let rateLimitError = JSONRPCError(
            message: "Rate limited: retry after 5s",
            code: HOPErrorCode.rateLimited,
            data: nil
        )

        let mockLoader = MockHOPFileLoader()
        mockLoader.uploadEventsPerAttempt = [
            [.onError(rateLimitError)],
            [
                .onProgress(.init(uploaded: 500, total: 500, uploadedHashes: [Data()])),
                .onFinished(.chunked(metadata: Data(repeating: 0xFF, count: 32)))
            ]
        ]

        let env = makeTestEnv(loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader))
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(
            makeUploadContent(),
            status: .outgoing(.new)
        )
        env.service.setup()

        let event = await awaitTerminalEvent(
            for: makeAttachmentId(message: message),
            in: env.service
        )

        if case .onComplete = event {
            // expected
        } else {
            Issue.record("Expected onComplete after rate-limit retry, got \(String(describing: event))")
        }

        #expect(mockLoader.uploadAttemptCount == 2)

        env.service.throttle()
    }

    @Test("deterministic HOP errors are not retried")
    func notAuthorizedNotRetried() async throws {
        let mockLoader = MockHOPFileLoader()
        mockLoader.uploadEventsPerAttempt = [
            [.onError(JSONRPCError(message: "Not authorized", code: 1_012, data: nil))]
        ]

        let env = makeTestEnv(loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader))
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(
            makeUploadContent(),
            status: .outgoing(.new)
        )
        env.service.setup()

        let event = await awaitTerminalEvent(
            for: makeAttachmentId(message: message),
            in: env.service
        )

        if case .onFailure = event {
            // expected
        } else {
            Issue.record("Expected onFailure, got \(String(describing: event))")
        }

        #expect(mockLoader.uploadAttemptCount == 1)

        env.service.throttle()
    }
}
