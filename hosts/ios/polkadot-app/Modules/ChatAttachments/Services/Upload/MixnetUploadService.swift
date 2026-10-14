import Foundation
import HandoffService
import Operation_iOS
import CommonService
import AsyncExtensions
import KeyDerivation
import SubstrateSdk
import NovaCrypto
import StructuredConcurrency
import Individuality

protocol AttachmentUploadingServicing: AttachmentLoadProgressProvidable, ApplicationServiceProtocol {}

struct UploadRetryPolicy: Sendable {
    let maxAttempts: Int
    let initialDelay: Duration

    /// Attempts are deliberately few: a retried chunk is re-encrypted with a fresh nonce, so the
    /// HOP node stores it as a new entry rather than deduplicating it. An entry nothing ever acks
    /// is promoted to permanent on-chain storage ~22h later at the sender's expense.
    static let `default` = UploadRetryPolicy(maxAttempts: 5, initialDelay: .seconds(2))
}

final class MixnetUploadService: @unchecked Sendable {
    static let streamRetryMaxAttempts = 5
    static let streamRetryInitialDelay: Duration = .seconds(2)

    let loaderFactory: HOPFileLoaderMaking
    let messageProviderFactory: ChatMessageDataProviderMaking
    let uploadContextFactory: UploadFileContextFactory
    let logger: LoggerProtocol

    let context: MixnetUploadContext
    let senderProvider: AttachmentsSenderProviding
    let allowanceManager: AllowanceManaging
    let retryPolicy: UploadRetryPolicy

    private var uploadTask: Task<Void, Never>?

    init(
        loaderFactory: HOPFileLoaderMaking,
        storageFacade: StorageFacadeProtocol,
        uploadContextFactory: UploadFileContextFactory,
        senderProvider: AttachmentsSenderProviding,
        allowanceManager: AllowanceManaging,
        retryPolicy: UploadRetryPolicy = .default,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.loaderFactory = loaderFactory
        self.senderProvider = senderProvider
        self.allowanceManager = allowanceManager
        self.retryPolicy = retryPolicy
        self.uploadContextFactory = uploadContextFactory

        let repositoryFactory = ChatMessageRepositoryFactory(storageFacade: storageFacade)

        messageProviderFactory = ChatMessageDataProviderFactory(
            repositoryFactory: repositoryFactory,
            operationQueue: operationQueue,
            logger: logger
        )

        let attachmentUpdateRepository = storageFacade.createRepository(
            mapper: AnyCoreDataMapper(AttachmentUploadingMapper())
        )

        context = MixnetUploadContext(
            repository: AnyDataProviderRepository(attachmentUpdateRepository),
            logger: logger
        )

        self.logger = logger
    }
}

extension MixnetUploadService: AttachmentUploadingServicing {
    func setup() {
        startUploading()
    }

    func throttle() {
        uploadTask?.cancel()
        cancelAllUploading()
    }

    func subscribeState(for attachmentId: AttachmentId) async -> AnyAsyncSequence<AttachmentProgressEvent?> {
        await context.subscribeState(for: attachmentId)
    }
}

private extension MixnetUploadService {
    func cancelAllUploading() {
        Task { [context] in
            await context.cancelAll()
        }
    }

    func runUploadAttempt(for uploadData: MixnetUploadData) async throws {
        guard let store = uploadContextFactory.createContext(
            attachmentId: uploadData.attachmentId
        ) else {
            logger.error("Failed to create upload context for \(uploadData.attachmentId.fileId)")
            return
        }

        let credentials = try await store.ensureUploadCredentials()

        let fileLoader = try loaderFactory.makeLoader(for: credentials.node)
        let recipients = try FileRecipients(ticket: credentials.ticket)

        let proofWallet = try await senderProvider.getWallet(for: uploadData.chatId)

        let accountId = try proofWallet.getRawPublicKey()
        try await allowanceManager.ensureCanSubmit(accountId: accountId, priority: .normal)

        let sender = try proofWallet.getMultiSigner()
        let proofProvider = SenderProofProvider(sender: sender) { data in
            try proofWallet.sign(data: data)
        }

        let uploadingStream = fileLoader.uploadFile(
            store: store,
            sender: proofProvider,
            recipients: recipients
        )

        try await markStallRegion("Uploading file") {
            for try await event in uploadingStream {
                try await self.handleUploadingEvent(
                    event,
                    uploadData: uploadData,
                    ticket: credentials.ticket,
                    node: credentials.node
                )
            }
        }
    }

    /// `retry_after_secs` from a `RateLimited` response is only rendered into the error message
    /// string — the node sends no structured `data` — so these retry on our own backoff rather
    /// than the delay the node suggests.
    static func isRetryableUploadError(_ error: Error) -> Bool {
        switch error {
        case JSONRPCEngineError.remoteCancelled,
             is URLError:
            true
        case let rpcError as JSONRPCError:
            rpcError.code == HOPErrorCode.poolFull || rpcError.code == HOPErrorCode.rateLimited
        default:
            false
        }
    }

    func performUploadingIfNeeded(for uploadData: MixnetUploadData) async {
        await context.processUploadData(
            for: uploadData
        ) { [logger, retryPolicy, weak self] in
            Task {
                do {
                    try await markStallActivity("Sending attachment") {
                        try await withRetry(
                            maxAttempts: retryPolicy.maxAttempts,
                            initialDelay: retryPolicy.initialDelay,
                            shouldRetry: { MixnetUploadService.isRetryableUploadError($0) },
                            operation: { [weak self] in
                                try await self?.runUploadAttempt(for: uploadData)
                            }
                        )
                    }

                    logger.debug("Task completed successfully")
                } catch {
                    guard !Task.isCancelled else {
                        return
                    }

                    logger.error("Task completed with error: \(error)")

                    await self?.context.handle(
                        uploadEvent: .onFailure(error),
                        attachmentId: uploadData.attachmentId
                    )
                }
            }
        }
    }

    func handleUploadingEvent(
        _ event: FileUploadingEvent,
        uploadData: MixnetUploadData,
        ticket: FileTicket,
        node: ChatRemoteMessageContent.NodeEndpoint
    ) async throws {
        switch event {
        case let .onProgress(progress):
            let fileId = uploadData.attachmentId.fileId
            logger
                .debug(
                    "\(fileId): \(progress.uploaded) out of \(progress.total) uploaded"
                )

            await context.handle(
                uploadEvent: .onProgress(
                    .init(
                        uploaded: progress.uploaded,
                        total: progress.total
                    )
                ),
                attachmentId: uploadData.attachmentId
            )
        case let .onFinished(finished):
            logger.debug("Finished uploading")

            await context.handle(
                uploadEvent: .onComplete(
                    .toPeer(
                        .init(
                            identifier: finished.entryHash,
                            claimTicket: ticket,
                            node: node
                        )
                    )
                ),
                attachmentId: uploadData.attachmentId
            )
        case let .onError(error):
            logger.error("Uploading failed: \(error)")

            throw error
        }
    }

    func startUploading() {
        uploadTask = Task { [messageProviderFactory, logger] in
            do {
                try await withRetry(
                    maxAttempts: MixnetUploadService.streamRetryMaxAttempts,
                    initialDelay: MixnetUploadService.streamRetryInitialDelay
                ) { [weak self] in
                    let stream = messageProviderFactory.subscribeMessages(
                        with: .newLocalDeviceOutgoingRemoteRichTextMessages()
                    )

                    logger.debug("Starting messages stream")

                    for try await messages in stream {
                        let uploadList = messages.flatMap { message in
                            MixnetUploadList.createUploadList(from: message)
                        }

                        for uploadData in uploadList {
                            await self?.performUploadingIfNeeded(for: uploadData)
                        }
                    }
                }
            } catch {
                guard !Task.isCancelled else {
                    return
                }

                logger.error("Task failed: \(error)")
            }
        }
    }
}
