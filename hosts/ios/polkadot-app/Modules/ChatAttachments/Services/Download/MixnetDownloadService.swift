import Foundation
import Operation_iOS
import HandoffService
import AsyncExtensions
import CommonService
import StructuredConcurrency

protocol AttachmentDownloadingServicing: AttachmentLoadProgressProvidable, ApplicationServiceProtocol {}

final class MixnetDownloadService {
    let loaderFactory: HOPFileLoaderMaking
    let hopNodeProvider: HOPNodeProviding
    let messageProviderFactory: ChatMessageDataProviderMaking
    let attachmentsStore: AttachmentStoring
    let downloadContextFactory: DownloadFileContextFactory
    let logger: LoggerProtocol

    let context: MixnetDownloadContext

    private var messagesTask: Task<Void, Never>?

    init(
        loaderFactory: HOPFileLoaderMaking,
        hopNodeProvider: HOPNodeProviding,
        storageFacade: StorageFacadeProtocol,
        attachmentsStore: AttachmentStoring,
        downloadContextFactory: DownloadFileContextFactory,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.loaderFactory = loaderFactory
        self.hopNodeProvider = hopNodeProvider
        self.attachmentsStore = attachmentsStore
        self.downloadContextFactory = downloadContextFactory

        let repositoryFactory = ChatMessageRepositoryFactory(storageFacade: storageFacade)

        messageProviderFactory = ChatMessageDataProviderFactory(
            repositoryFactory: repositoryFactory,
            operationQueue: operationQueue,
            logger: logger
        )

        context = MixnetDownloadContext(logger: logger)

        self.logger = logger
    }
}

extension MixnetDownloadService: AttachmentDownloadingServicing {
    func setup() {
        subscribeMessages()
    }

    func throttle() {
        messagesTask?.cancel()
        cancelAllDownloading()
    }

    func subscribeState(for attachmentId: AttachmentId) async -> AnyAsyncSequence<AttachmentProgressEvent?> {
        await context.subscribeState(for: attachmentId)
    }
}

private extension MixnetDownloadService {
    func cancelAllDownloading() {
        Task { [context] in
            await context.cancelAll()
        }
    }

    func performDownloadingIfNeeded(for downloadData: MixnetDownloadData) async {
        await context.processDownloadData(
            for: downloadData
        ) { [logger, loaderFactory, downloadContextFactory, hopNodeProvider, weak self] in
            Task {
                do {
                    try await markStallActivity("Receiving attachment") {
                        let fileVariant = downloadData.fileVariant

                        guard hopNodeProvider.isNodeAllowed(fileVariant.node) else {
                            logger.error("Untrusted HOP node rejected: \(fileVariant.node)")
                            throw HOPFileLoaderError.untrustedNode
                        }

                        let fileDownloadContext = downloadContextFactory.createContext(
                            entryHash: fileVariant.identifier,
                            filename: fileVariant.filename
                        )

                        let claimer = try FileClaimer(ticket: fileVariant.claimTicket)
                        let loader = try loaderFactory.makeLoader(for: fileVariant.node)

                        let stream = loader.downloadFile(
                            using: fileVariant.identifier,
                            claimer: claimer,
                            store: fileDownloadContext
                        )

                        try await markStallRegion("Downloading file") {
                            for try await event in stream {
                                if case let .onError(error) = event {
                                    throw error
                                }

                                await self?.handleDownloadingEvent(event, downloadData: downloadData)
                            }
                        }
                    }

                    logger.debug("Download task completed")
                } catch {
                    guard !Task.isCancelled else {
                        return
                    }

                    logger.error("Download task completed with error: \(error)")

                    await self?.handleDownloadingEvent(.onError(error), downloadData: downloadData)
                }
            }
        }
    }

    func handleDownloadingEvent(_ event: FileDownloadingEvent, downloadData: MixnetDownloadData) async {
        switch event {
        case let .onProgress(progress):
            let fileId = downloadData.attachmentId.fileId
            logger.debug(
                "\(fileId): \(progress.downloaded) out of \(progress.total) downloaded"
            )

            await context.handle(
                downloadEvent: .onProgress(
                    .init(
                        loaded: progress.downloaded,
                        total: progress.total
                    )
                ),
                attachmentId: downloadData.attachmentId
            )

        case .onFinished:
            logger.debug("Finished downloading")

            await context.handle(
                downloadEvent: .onComplete,
                attachmentId: downloadData.attachmentId
            )

        case let .onError(error):
            logger.error("Downloading failed: \(error)")
            await context.handle(
                downloadEvent: .onFailure(error),
                attachmentId: downloadData.attachmentId
            )
        }
    }

    func subscribeMessages() {
        messagesTask = Task { [logger, attachmentsStore, messageProviderFactory] in
            do {
                let stream = messageProviderFactory.subscribeMessages(with: .incomingRemoteRichTextMessages())

                for try await messages in stream {
                    let downloadList = messages.flatMap { message in
                        MixnetDownloadList.createDownloadList(from: message)
                    }
                    .filter { !attachmentsStore.hasFile(for: $0.fileVariant.filename) }

                    for downloadData in downloadList {
                        await performDownloadingIfNeeded(for: downloadData)
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
