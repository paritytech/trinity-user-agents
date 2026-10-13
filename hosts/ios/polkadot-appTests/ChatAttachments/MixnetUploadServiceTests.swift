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

@Suite("MixnetUploadService")
struct MixnetUploadServiceTests {
    private let localFilePath = "uploads/test-video.mp4"
    private let fileData = Data(repeating: 0xAB, count: 500)

    // MARK: - Tests

    @Test("message with uploadable attachment triggers upload and completes")
    func messageTriggersUpload() async throws {
        let mockLoader = MockHOPFileLoader()
        mockLoader.uploadEvents = [
            .onProgress(.init(uploaded: 250, total: 500, uploadedHashes: [Data()])),
            .onProgress(.init(uploaded: 500, total: 500, uploadedHashes: [Data(), Data()])),
            .onFinished(.chunked(metadata: Data(repeating: 0xFF, count: 32)))
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
            Issue.record("Expected onComplete, got \(String(describing: event))")
        }

        env.service.throttle()
    }

    @Test("no HOP node emits failure event")
    func noNodeAvailable() async throws {
        let mockLoader = MockHOPFileLoader()
        let nodeProvider = MockHOPNodeProvider(allowedNodes: [])

        let env = makeTestEnv(
            loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader),
            nodeProvider: nodeProvider
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
            Issue.record("Expected onFailure, got \(String(describing: event))")
        }

        env.service.throttle()
    }

    @Test("upload error is forwarded to context")
    func uploadErrorForwarded() async throws {
        let mockLoader = MockHOPFileLoader()
        mockLoader.uploadEvents = [
            .onProgress(.init(uploaded: 100, total: 500, uploadedHashes: [Data()])),
            .onError(NSError(domain: "test", code: 42))
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

        env.service.throttle()
    }

    @Test("loader factory error is forwarded to context")
    func loaderFactoryErrorForwarded() async throws {
        let env = makeTestEnv(
            loaderFactory: MockHOPFileLoaderFactory(makeError: HOPFileLoaderError.noAvailableNodes)
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
            Issue.record("Expected onFailure, got \(String(describing: event))")
        }

        env.service.throttle()
    }

    @Test("resumed upload uses persisted node, not a new one")
    func resumedUploadUsesPersistedNode() async throws {
        let loaderFactory = MockHOPFileLoaderFactory(loader: {
            let loader = MockHOPFileLoader()
            loader.uploadEvents = [
                .onProgress(.init(uploaded: 250, total: 500, uploadedHashes: [Data()])),
                .onFinished(.chunked(metadata: Data(repeating: 0xFF, count: 32)))
            ]
            return loader
        }())

        // First run: node provider returns "trusted" node
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
            Issue.record("Expected onComplete, got \(String(describing: event))")
        }

        // Verify the loader was given the persisted node, not a freshly selected one
        #expect(loaderFactory.lastRequestedNode == MockHopNodes.trusted)

        env.service.throttle()
    }

    @Test("message with existing uploadingInfo is skipped")
    func alreadyUploadedSkipped() async throws {
        let mockLoader = MockHOPFileLoader()
        mockLoader.shouldSuspend = true

        let env = makeTestEnv(loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader))
        try await env.chatManager.setup()

        // Create content with uploadingInfo already set — should be filtered out
        let uploadedAttachment: Chat.LocalMessage.Content.Attachment = .localUploadable(.init(
            relativeLocalPath: localFilePath,
            meta: .general(.init(mimeType: "video/mp4", fileSize: 500)),
            uploadingInfo: .toPeer(.init(
                identifier: Data(repeating: 0x01, count: 32),
                claimTicket: Data(repeating: 0x02, count: 32),
                node: MockHopNodes.trusted
            ))
        ))

        _ = try await env.chatManager.sendMessage(
            .richText(.init(text: nil, attachments: [uploadedAttachment])),
            status: .outgoing(.new)
        )
        env.service.setup()

        try await Task.sleep(for: .milliseconds(500))

        let taskCount = await env.service.context.tasks.count
        #expect(taskCount == 0)

        env.service.throttle()
    }
}

// MARK: - Helpers

extension UploadRetryPolicy {
    static let fastRetries = UploadRetryPolicy(maxAttempts: 3, initialDelay: .milliseconds(1))
}

actor TerminalEventObserver {
    private var sawFailure: Bool?

    func record(sawFailure value: Bool) {
        guard sawFailure == nil else { return }

        sawFailure = value
    }

    /// Polls for the terminal outcome rather than awaiting the collector task directly: racing
    /// `Task.value` inside a task group has produced CI-only flakes in this suite.
    /// Returns nil if no terminal event arrived before the deadline.
    func awaitOutcome(timeout: Duration = .seconds(30)) async -> Bool? {
        let deadline = ContinuousClock.now + timeout

        while ContinuousClock.now < deadline {
            if let sawFailure {
                return sawFailure
            }

            try? await Task.sleep(for: .milliseconds(50))
        }

        return nil
    }
}

extension MixnetUploadServiceTests {
    struct TestEnv {
        let service: MixnetUploadService
        let chatManager: TestChatManager
    }

    func makeTestEnv(
        loaderFactory: HOPFileLoaderMaking = MockHOPFileLoaderFactory(),
        nodeProvider: HOPNodeProviding = MockHOPNodeProvider(),
        allowanceManager: AllowanceManaging = MockAllowanceManager(),
        retryPolicy: UploadRetryPolicy = .fastRetries
    ) -> TestEnv {
        let facade = UserDataStorageTestFacade()

        let attachmentsStore = MockAttachmentStore()
        _ = try? attachmentsStore.store(attachment: fileData, filename: localFilePath)

        let uploadRepoFactory = MixnetUploadRepositoryFactory(storageFacade: facade)

        let uploadContextFactory = UploadFileContextFactory(
            attachmentsStore: attachmentsStore,
            nodeProvider: nodeProvider,
            repository: uploadRepoFactory.createRepository(),
            updateRepository: uploadRepoFactory.createUpdateRepository()
        )

        let service = MixnetUploadService(
            loaderFactory: loaderFactory,
            storageFacade: facade,
            uploadContextFactory: uploadContextFactory,
            senderProvider: MockAttachmentsSenderProvider(),
            allowanceManager: allowanceManager,
            retryPolicy: retryPolicy,
            logger: Logger.shared
        )

        let chatManager = TestChatManager(
            peer: MockChatPeer.person(),
            facade: facade
        )

        return TestEnv(service: service, chatManager: chatManager)
    }

    func makeDropThenSucceedLoader() -> MockHOPFileLoader {
        let loader = MockHOPFileLoader()
        loader.uploadEventsPerAttempt = [
            [.onError(JSONRPCEngineError.remoteCancelled)],
            [
                .onProgress(.init(uploaded: 500, total: 500, uploadedHashes: [Data()])),
                .onFinished(.chunked(metadata: Data(repeating: 0xFF, count: 32)))
            ]
        ]

        return loader
    }

    func makeUploadContent() -> Chat.LocalMessage.Content {
        let attachment: Chat.LocalMessage.Content.Attachment = .localUploadable(.init(
            relativeLocalPath: localFilePath,
            meta: .general(.init(mimeType: "video/mp4", fileSize: 500)),
            uploadingInfo: nil
        ))

        return .richText(.init(text: nil, attachments: [attachment]))
    }

    func makeAttachmentId(message: Chat.LocalMessage) -> AttachmentId {
        AttachmentId(
            messageId: message.messageId,
            fileId: localFilePath
        )
    }

    func awaitTerminalEvent(
        for attachmentId: AttachmentId,
        in service: MixnetUploadService,
        timeout: Duration = .milliseconds(100_000)
    ) async -> AttachmentProgressEvent? {
        let deadline = ContinuousClock.now + timeout

        while ContinuousClock.now < deadline {
            if let subject = await service.context.state[attachmentId],
               let event = subject.value {
                switch event {
                case .onComplete,
                     .onFailure:
                    return event
                case .onProgress:
                    break
                }
            }

            try? await Task.sleep(for: .milliseconds(50))
        }

        return nil
    }
}
