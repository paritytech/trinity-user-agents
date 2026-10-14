import Foundation
import HandoffService
import AsyncExtensions
import Operation_iOS
import StructuredConcurrency
import Testing
import SDKLogger

@testable import polkadot_app

@Suite("MixnetDownloadService")
struct MixnetDownloadServiceTests {
    // MARK: - Tests

    @Test("message with downloadable attachment triggers download and completes")
    func messageTriggersDownload() async throws {
        let mockLoader = MockHOPFileLoader()
        mockLoader.downloadEvents = [
            .onProgress(.init(downloaded: 500, total: 1_000)),
            .onFinished(Data(repeating: 0xAA, count: 32))
        ]

        let env = makeTestEnv(loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader))
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(makeRichTextContent(), status: .incoming(.new))
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

    @Test("untrusted node emits failure event")
    func untrustedNodeRejects() async throws {
        let mockLoader = MockHOPFileLoader()
        let nodeProvider = MockHOPNodeProvider(allowedNodes: [])

        let env = makeTestEnv(
            loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader),
            nodeProvider: nodeProvider
        )
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(makeRichTextContent(), status: .incoming(.new))
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

    @Test("already downloaded file is skipped")
    func alreadyDownloadedSkipped() async throws {
        let attachmentsStore = MockAttachmentStore()
        let mockLoader = MockHOPFileLoader()
        mockLoader.shouldSuspend = true

        let filename = Data(repeating: 0xAA, count: 32).toHex() + ".mp4"
        try attachmentsStore.store(attachment: Data("existing".utf8), filename: filename)

        let env = makeTestEnv(
            loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader),
            attachmentsStore: attachmentsStore
        )
        try await env.chatManager.setup()

        _ = try await env.chatManager.sendMessage(makeRichTextContent(), status: .incoming(.new))
        env.service.setup()

        try await Task.sleep(for: .milliseconds(500))

        let taskCount = await env.service.context.tasks.count
        #expect(taskCount == 0)

        env.service.throttle()
    }

    @Test("download error is forwarded to context")
    func downloadErrorForwarded() async throws {
        let mockLoader = MockHOPFileLoader()
        mockLoader.downloadEvents = [
            .onProgress(.init(downloaded: 100, total: 1_000)),
            .onError(NSError(domain: "test", code: 42))
        ]

        let env = makeTestEnv(loaderFactory: MockHOPFileLoaderFactory(loader: mockLoader))
        try await env.chatManager.setup()

        let message = try await env.chatManager.sendMessage(makeRichTextContent(), status: .incoming(.new))
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
}

// MARK: - Helpers

extension MixnetDownloadServiceTests {
    struct TestEnv {
        let service: MixnetDownloadService
        let chatManager: TestChatManager
    }

    private func makeTestEnv(
        loaderFactory: HOPFileLoaderMaking = MockHOPFileLoaderFactory(),
        nodeProvider: HOPNodeProviding = MockHOPNodeProvider(),
        attachmentsStore: MockAttachmentStore = MockAttachmentStore()
    ) -> TestEnv {
        let facade = UserDataStorageTestFacade()
        let downloadRepoFactory = MixnetDownloadRepositoryFactory(storageFacade: facade)

        let downloadContextFactory = DownloadFileContextFactory(
            attachmentsStore: attachmentsStore,
            repository: downloadRepoFactory.createRepository(),
            chunkIndexRepository: downloadRepoFactory.createChunkIndexRepository()
        )

        let service = MixnetDownloadService(
            loaderFactory: loaderFactory,
            hopNodeProvider: nodeProvider,
            storageFacade: facade,
            attachmentsStore: attachmentsStore,
            downloadContextFactory: downloadContextFactory,
            logger: Logger.shared
        )

        let chatManager = TestChatManager(
            peer: MockChatPeer.person(),
            facade: facade
        )

        return TestEnv(service: service, chatManager: chatManager)
    }

    private func makeRichTextContent(
        fileId: Data = Data(repeating: 0xAA, count: 32),
        ticket: Data = Data(repeating: 0xBB, count: 32),
        node: ChatRemoteMessageContent.NodeEndpoint = MockHopNodes.trusted
    ) -> Chat.LocalMessage.Content {
        let attachment: Chat.LocalMessage.Content.Attachment = .remoteDownloadable(
            .p2pMixnet(.init(
                identifier: fileId,
                claimTicket: ticket,
                node: node,
                meta: .general(.init(mimeType: "video/mp4", fileSize: 1_000))
            ))
        )

        return .richText(.init(text: nil, attachments: [attachment]))
    }

    private func makeAttachmentId(
        message: Chat.LocalMessage,
        fileId: Data = Data(repeating: 0xAA, count: 32)
    ) -> AttachmentId {
        AttachmentId(
            messageId: message.messageId,
            fileId: fileId.toHex() + ".mp4"
        )
    }

    private func awaitTerminalEvent(
        for attachmentId: AttachmentId,
        in service: MixnetDownloadService,
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
                    // Intermediate value — keep waiting for a terminal event so the
                    // poll can't observe progress before completion/failure lands.
                    break
                }
            }

            try? await Task.sleep(for: .milliseconds(50))
        }

        return nil
    }
}
