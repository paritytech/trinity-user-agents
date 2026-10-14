import Foundation
import HandoffService
import Operation_iOS
import SDKLogger

actor DownloadFileContext {
    let entryHash: FileHash
    let filename: String
    let attachmentsStore: AttachmentStoring
    let repository: AnyDataProviderRepository<MixnetDownload>
    let chunkIndexRepository: AnyDataProviderRepository<MixnetDownloadChunkIndex>
    let fileManager: FileManager
    let logger: LoggerProtocol

    private var fileHandle: FileHandle?

    init(
        entryHash: FileHash,
        filename: String,
        attachmentsStore: AttachmentStoring,
        repository: AnyDataProviderRepository<MixnetDownload>,
        chunkIndexRepository: AnyDataProviderRepository<MixnetDownloadChunkIndex>,
        fileManager: FileManager = FileManager.default,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.entryHash = entryHash
        self.filename = filename
        self.attachmentsStore = attachmentsStore
        self.repository = repository
        self.chunkIndexRepository = chunkIndexRepository
        self.fileManager = fileManager
        self.logger = logger
    }
}

extension DownloadFileContext: DownloadFileContextProtocol {
    nonisolated var identifier: String { entryHash.toHex() }

    func saveEntry(_ entry: DownloadedEntry) async throws {
        switch entry {
        case let .inline(fileData):
            try saveInlineFile(fileData)

            let model = MixnetDownload(
                entryHashHex: identifier,
                entryType: .inline,
                lastChunkIndex: -1,
                totalChunks: 0,
                metadata: nil,
                downloadedBytes: Int64(fileData.count)
            )

            try await repository.saveOperation({ [model] }, { [] }).asyncExecute()
        case let .chunked(metadata, totalChunks):
            let model = MixnetDownload(
                entryHashHex: identifier,
                entryType: .metadata,
                lastChunkIndex: -1,
                totalChunks: Int32(totalChunks),
                metadata: metadata,
                downloadedBytes: 0
            )

            try await repository.saveOperation({ [model] }, { [] }).asyncExecute()
        }
    }

    func fetchResumeInfo() async throws -> ResumeDownloadInfo? {
        let id = identifier

        let optDownload = try await repository.fetchOperation(
            by: { id },
            options: RepositoryFetchOptions()
        )
        .asyncExecute()

        guard let download = optDownload else {
            return nil
        }

        switch download.entryType {
        case .inline:
            return try await inlineResumeInfo(for: download)
        case .metadata:
            return chunkedResumeInfo(for: download)
        }
    }

    func appendChunk(_ data: Data, at index: Int) async throws {
        let handle = try getOrCreateHandle()
        let currentEnd = try handle.seekToEnd()

        let update = MixnetDownloadChunkIndex(
            entryHashHex: identifier,
            lastChunkIndex: Int32(index),
            downloadedBytes: Int64(currentEnd) + Int64(data.count)
        )

        try await chunkIndexRepository.saveOperation({ [update] }, { [] }).asyncExecute()

        try handle.write(contentsOf: data)
        try handle.synchronize()

        logger.debug("Appended chunk: \(index)")
    }

    func finishDownloading(_ fullFileDownloaded: Bool) async throws {
        closeHandle()

        guard fullFileDownloaded else {
            return
        }

        try attachmentsStore.moveFile(from: partialFilename, to: filename)

        let id = identifier
        try await repository.saveOperation({ [] }, { [id] }).asyncExecute()
    }
}

private extension DownloadFileContext {
    var partialFilename: String {
        filename + ".part"
    }

    func saveInlineFile(_ fileData: Data) throws {
        try attachmentsStore.store(attachment: fileData, filename: partialFilename)
    }

    func inlineResumeInfo(for download: MixnetDownload) async throws -> ResumeDownloadInfo? {
        let actualFileSize = partialFileSize()

        guard actualFileSize >= Int(download.downloadedBytes) else {
            // The inline file write was interrupted — drop the state and restart fresh.
            logger.warning(
                "Inline download state mismatch: DB expects \(download.downloadedBytes) bytes "
                    + "but file has \(actualFileSize). Restarting"
            )

            let id = identifier
            try await repository.saveOperation({ [] }, { [id] }).asyncExecute()

            return nil
        }

        return .inline(downloadedBytes: Int(download.downloadedBytes))
    }

    func chunkedResumeInfo(for download: MixnetDownload) -> ResumeDownloadInfo? {
        guard let metadata = download.metadata else {
            return nil
        }

        let actualFileSize = partialFileSize()

        let lastChunkIndex: Int?
        let downloadedBytes: Int

        if download.downloadedBytes > Int64(actualFileSize) {
            // DB is ahead of the file — the last chunk write was interrupted.
            // Roll back to the previous chunk so it gets re-downloaded.
            lastChunkIndex = download.lastChunkIndex > 0 ? Int(download.lastChunkIndex - 1) : nil
            downloadedBytes = actualFileSize

            logger.warning(
                "Download state mismatch: DB expects \(download.downloadedBytes) bytes "
                    + "but file has \(actualFileSize). Rolling back to chunk \(String(describing: lastChunkIndex))"
            )
        } else {
            lastChunkIndex = download.lastChunkIndex >= 0 ? Int(download.lastChunkIndex) : nil
            downloadedBytes = actualFileSize
        }

        logger.debug("Last chunk index: \(String(describing: lastChunkIndex)), downloaded bytes: \(downloadedBytes)")

        let chunked = ResumeDownloadInfo.Chunked(
            metadata: metadata,
            lastChunkIndex: lastChunkIndex,
            downloadedBytes: downloadedBytes
        )

        return .chunked(chunked)
    }

    func getOrCreateHandle() throws -> FileHandle {
        if let fileHandle {
            return fileHandle
        }

        let url = attachmentsStore.fileURL(for: partialFilename)

        if !attachmentsStore.hasFile(for: partialFilename) {
            try attachmentsStore.createEmptyFile(for: partialFilename)
        }

        let handle = try FileHandle(forWritingTo: url)
        fileHandle = handle

        return handle
    }

    func partialFileSize() -> Int {
        let url = attachmentsStore.fileURL(for: partialFilename)
        let attributes = try? fileManager.attributesOfItem(atPath: url.path)
        return (attributes?[.size] as? Int) ?? 0
    }

    func closeHandle() {
        if let fileHandle {
            try? fileHandle.close()
            self.fileHandle = nil
        }
    }
}
