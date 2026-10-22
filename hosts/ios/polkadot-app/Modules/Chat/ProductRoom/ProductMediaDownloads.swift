import Foundation

/// Takes the photos and the videos that the contacts of the chat product send: the core opens each HOP file from the
/// nodes of the app, and the file goes to the store of the downloads of the native chat. The media views of the
/// native chat draw it with its BlurHash preview and the progress of the download, as they draw a file of a native
/// chat.
actor ProductMediaDownloads {
    static let shared = ProductMediaDownloads()

    nonisolated let progress = ProductMediaProgress()

    private let files: ChatHopFileOpening
    private let store: AttachmentStoring?
    private let logger: LoggerProtocol
    private var running: Set<AttachmentId> = []

    init(
        files: ChatHopFileOpening = TrUAPIChatHopFiles(),
        store: AttachmentStoring? = AttachmentStore.dowloads(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.files = files
        self.store = store
        self.logger = logger
    }

    /// Takes each file of the messages that the store of the downloads does not hold yet.
    func download(filesOf messages: [Chat.LocalMessage]) {
        for message in messages {
            for file in Self.remoteFiles(of: message) {
                download(file, messageId: message.messageId)
            }
        }
    }

    /// Keeps the bytes of a file that the user sent, so the message of the user draws without a download.
    func keep(_ bytes: Data, for file: ChatRemoteMessageContent.FileVariant) {
        do {
            try store?.store(attachment: bytes, filename: file.filename)
        } catch {
            logger.warning("A sent file was not kept: \(error)")
        }
    }
}

private extension ProductMediaDownloads {
    static func remoteFiles(of message: Chat.LocalMessage) -> [ChatRemoteMessageContent.FileVariant] {
        switch message.content {
        case let .richText(richText):
            richText.attachments?.compactMap { attachment in
                if case let .remoteDownloadable(variant) = attachment { variant } else { nil }
            } ?? []
        case let .reply(reply):
            reply.ownContent.attachments ?? []
        default:
            []
        }
    }

    func download(_ file: ChatRemoteMessageContent.FileVariant, messageId: Chat.MessageId) {
        let attachmentId = AttachmentId(messageId: messageId, fileId: file.filename)

        guard let store, !store.hasFile(for: file.filename), !running.contains(attachmentId),
              case let .wssUrl(node) = file.node else {
            return
        }

        running.insert(attachmentId)
        let resource = ChatHopResource(
            identifier: file.identifier,
            ticket: file.claimTicket,
            node: node,
            size: UInt64(file.meta.fileSize)
        )

        Task { [files, progress, logger] in
            await progress.report(.onProgress(.init(loaded: 0, total: Int(file.meta.fileSize))), for: attachmentId)

            // The core reports the bytes received while the file arrives. The reports reach the progress ring in
            // their order, and before the end of the download.
            let (received, receiving) = AsyncStream<AttachmentProgressEvent.Progress>.makeStream(
                bufferingPolicy: .bufferingNewest(1)
            )
            let reporting = Task {
                for await loaded in received {
                    await progress.report(.onProgress(loaded), for: attachmentId)
                }
            }

            do {
                let opened = try await files.open(resource) { loaded, total in
                    receiving.yield(.init(loaded: Int(clamping: loaded), total: Int(clamping: total)))
                }
                receiving.finish()
                await reporting.value
                try store.store(attachment: opened.bytes, filename: file.filename)
                try? await files.ack(resource, entries: opened.entries)
                await progress.report(.onComplete, for: attachmentId)
            } catch {
                receiving.finish()
                await reporting.value
                logger.warning("A file of the chat product did not open: \(error)")
                await progress.report(.onFailure(error), for: attachmentId)
            }

            self.finish(attachmentId)
        }
    }

    func finish(_ attachmentId: AttachmentId) {
        running.remove(attachmentId)
    }
}
