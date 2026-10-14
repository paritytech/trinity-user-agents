import Foundation
import AsyncExtensions

public final class HandoffFileLoader {
    public let config: HandoffFileLoadConfig
    public let service: HandoffServicing

    public init(service: HandoffServicing, config: HandoffFileLoadConfig = HandoffFileLoadConfig()) {
        self.service = service
        self.config = config
    }
}

extension HandoffFileLoader: HandoffFileLoading {
    public func uploadFile(
        store: UploadFileContextProtocol,
        sender: SenderProofProviding,
        recipients: FileRecipients
    ) -> AnyAsyncSequence<FileUploadingEvent> {
        AsyncStream { continuation in
            let task = Task {
                do {
                    let resumeInfo = try await store.fetchResumeInfo()

                    let finished: FileUploadingEvent.Finished =
                        if resumeInfo.progress == nil,
                        resumeInfo.fileSize <= self.config.inlineThreshold {
                            try await self.uploadInline(
                                resumeInfo: resumeInfo,
                                store: store,
                                sender: sender,
                                recipients: recipients,
                                continuation: continuation
                            )
                        } else {
                            try await self.uploadChunked(
                                resumeInfo: resumeInfo,
                                store: store,
                                sender: sender,
                                recipients: recipients,
                                continuation: continuation
                            )
                        }

                    try await store.finishUploading(true)

                    continuation.yield(.onFinished(finished))
                    continuation.finish()
                } catch {
                    try? await store.finishUploading(false)

                    guard !Task.isCancelled else { return }

                    continuation.yield(.onError(error))
                    continuation.finish()
                }
            }

            continuation.onTermination = { _ in
                task.cancel()
            }
        }
        .eraseToAnyAsyncSequence()
    }

    public func downloadFile(
        using entryHash: FileHash,
        claimer: FileClaimer,
        store: DownloadFileContextProtocol
    ) -> AnyAsyncSequence<FileDownloadingEvent> {
        AsyncStream { continuation in
            let task = Task {
                do {
                    let entry = try await self.deriveDownloadEntry(
                        for: entryHash,
                        claimer: claimer,
                        store: store
                    )

                    guard let entry else {
                        continuation.yield(.onError(FileDownloadingError.noEntry(entryHash)))
                        continuation.finish()
                        return
                    }

                    switch entry {
                    case let .inline(downloadedBytes):
                        self.resumeInlineDownloading(
                            downloadedBytes: downloadedBytes,
                            continuation: continuation
                        )
                    case let .chunked(state):
                        try await self.resumeChunkedDownloading(
                            state: state,
                            claimer: claimer,
                            store: store,
                            continuation: continuation
                        )
                    }

                    try await store.finishDownloading(true)

                    continuation.yield(.onFinished(entryHash))
                    continuation.finish()
                } catch {
                    try? await store.finishDownloading(false)

                    guard !Task.isCancelled else { return }

                    continuation.yield(.onError(error))
                    continuation.finish()
                }
            }

            continuation.onTermination = { _ in
                task.cancel()
            }
        }
        .eraseToAnyAsyncSequence()
    }

    public func uploadBlob(
        _ data: Data,
        sender: SenderProofProviding,
        recipients: FileRecipients
    ) async throws -> FileHash {
        guard data.count <= config.inlineThreshold else {
            throw FileUploadingError.inlineLimitExceeded(size: data.count, limit: config.inlineThreshold)
        }

        let envelope = VersionedUploadedFile.v1(.inline(data))
        let encodedEntry = try envelope.scaleEncoded()
        let encryptedEntry = try recipients.encryptor.encrypt(encodedEntry)

        let submittedData = try await service.submitData(
            encryptedEntry,
            from: sender,
            recipients: recipients.pubKeys
        )

        return submittedData.hash
    }

    public func downloadBlob(
        _ hash: FileHash,
        claimer: FileClaimer,
        onConfirm: @escaping HandoffBlobConfirmClosure
    ) async throws {
        guard
            let encryptedEntry = try await service.claimData(
                by: hash,
                recipient: claimer.proofProvider
            ) else {
            throw FileDownloadingError.noEntry(hash)
        }

        let decryptedEntry = try claimer.decryptor.decrypt(encryptedEntry)
        let envelope = try VersionedUploadedFile.scaleDecode(from: decryptedEntry)

        // Inline-only by design: a chunked payload would let a blob ride the
        // unbounded chunk transport, so it is rejected without fetching chunks.
        guard case let .v1(.inline(blobData)) = envelope else {
            throw FileDownloadingError.unexpectedChunkedPayload(hash)
        }

        try await onConfirm(blobData)

        try await service.acknowledgeReceivedData(
            by: hash,
            recipient: claimer.proofProvider
        )
    }
}

// MARK: - Upload

private extension HandoffFileLoader {
    func uploadInline(
        resumeInfo: ResumeUploadInfo,
        store: UploadFileContextProtocol,
        sender: SenderProofProviding,
        recipients: FileRecipients,
        continuation: AsyncStream<FileUploadingEvent>.Continuation
    ) async throws -> FileUploadingEvent.Finished {
        let fileSize = resumeInfo.fileSize
        let fileData = try await store.fetchChunk(after: 0, size: fileSize)

        let envelope = VersionedUploadedFile.v1(.inline(fileData))
        let encodedEntry = try envelope.scaleEncoded()
        let encryptedEntry = try recipients.encryptor.encrypt(encodedEntry)

        let submittedData = try await service.submitData(
            encryptedEntry,
            from: sender,
            recipients: recipients.pubKeys
        )

        continuation.yield(.onProgress(.init(
            uploaded: fileSize,
            total: fileSize,
            uploadedHashes: []
        )))

        return .inline(file: submittedData.hash)
    }

    func uploadChunked(
        resumeInfo: ResumeUploadInfo,
        store: UploadFileContextProtocol,
        sender: SenderProofProviding,
        recipients: FileRecipients,
        continuation: AsyncStream<FileUploadingEvent>.Continuation
    ) async throws -> FileUploadingEvent.Finished {
        let hashes = try await uploadChunks(
            resumeInfo: resumeInfo,
            store: store,
            sender: sender,
            recipients: recipients,
            continuation: continuation
        )

        let chunkedFile = ChunkedFile(
            totalSize: UInt64(resumeInfo.fileSize),
            chunks: hashes
        )

        let envelope = VersionedUploadedFile.v1(.chunked(chunkedFile))
        let metadata = try envelope.scaleEncoded()
        let encryptedMetadata = try recipients.encryptor.encrypt(metadata)

        let submittedData = try await service.submitData(
            encryptedMetadata,
            from: sender,
            recipients: recipients.pubKeys
        )

        return .chunked(metadata: submittedData.hash)
    }

    func uploadChunks(
        resumeInfo: ResumeUploadInfo,
        store: UploadFileContextProtocol,
        sender: SenderProofProviding,
        recipients: FileRecipients,
        continuation: AsyncStream<FileUploadingEvent>.Continuation
    ) async throws -> [Data] {
        let fileSize = resumeInfo.fileSize
        var hashes = resumeInfo.progress?.uploadedHashes ?? []
        var totalUploaded = resumeInfo.progress?.uploadedSize ?? 0

        if totalUploaded > 0 {
            continuation.yield(.onProgress(.init(
                uploaded: totalUploaded,
                total: fileSize,
                uploadedHashes: hashes
            )))
        }

        while totalUploaded < fileSize {
            try Task.checkCancellation()

            let remaining = fileSize - totalUploaded
            let currentChunkSize = min(config.chunkSize, remaining)
            let chunk = try await store.fetchChunk(
                after: totalUploaded,
                size: currentChunkSize
            )

            let encryptedChunk = try recipients.encryptor.encrypt(chunk)
            let submittedData = try await service.submitData(
                encryptedChunk,
                from: sender,
                recipients: recipients.pubKeys
            )

            hashes.append(submittedData.hash)
            totalUploaded += chunk.count

            try await store.saveUploadedChunk(
                submittedData.hash,
                uploadedSize: Int64(totalUploaded)
            )

            continuation.yield(.onProgress(.init(
                uploaded: totalUploaded,
                total: fileSize,
                uploadedHashes: hashes
            )))
        }

        return hashes
    }
}

// MARK: - Download

private extension HandoffFileLoader {
    enum DownloadEntry {
        struct ChunkedState {
            let metadata: DownloadFileMetadata
            let lastChunkIndex: Int?
            let downloadedBytes: Int
        }

        case inline(downloadedBytes: Int)
        case chunked(ChunkedState)
    }

    func deriveDownloadEntry(
        for entryHash: FileHash,
        claimer: FileClaimer,
        store: DownloadFileContextProtocol
    ) async throws -> DownloadEntry? {
        if let resumeInfo = try await store.fetchResumeInfo() {
            return try makeDownloadEntry(from: resumeInfo)
        }

        guard
            let encryptedEntry = try await service.claimData(
                by: entryHash,
                recipient: claimer.proofProvider
            ) else {
            return nil
        }

        let decryptedEntry = try claimer.decryptor.decrypt(encryptedEntry)
        let envelope = try VersionedUploadedFile.scaleDecode(from: decryptedEntry)

        let entry = try await persistDownloadEntry(envelope, decryptedEntry: decryptedEntry, store: store)

        try await service.acknowledgeReceivedData(
            by: entryHash,
            recipient: claimer.proofProvider
        )

        return entry
    }

    // The ack MUST follow durable persistence: the ticket-derived key is the sole
    // recipient, so acking removes the entry and the data can't be re-claimed.
    func persistDownloadEntry(
        _ envelope: VersionedUploadedFile,
        decryptedEntry: Data,
        store: DownloadFileContextProtocol
    ) async throws -> DownloadEntry {
        switch envelope {
        case let .v1(.inline(fileData)):
            try await store.saveEntry(.inline(fileData: fileData))

            return .inline(downloadedBytes: fileData.count)
        case let .v1(.chunked(chunkedFile)):
            try await store.saveEntry(.chunked(
                metadata: decryptedEntry,
                totalChunks: chunkedFile.chunks.count
            ))

            let state = DownloadEntry.ChunkedState(
                metadata: DownloadFileMetadata(
                    totalSize: chunkedFile.totalSize,
                    chunkHashes: chunkedFile.chunks
                ),
                lastChunkIndex: nil,
                downloadedBytes: 0
            )

            return .chunked(state)
        }
    }

    func makeDownloadEntry(from resumeInfo: ResumeDownloadInfo) throws -> DownloadEntry {
        switch resumeInfo {
        case let .inline(downloadedBytes):
            return .inline(downloadedBytes: downloadedBytes)
        case let .chunked(chunked):
            let envelope = try VersionedUploadedFile.scaleDecode(from: chunked.metadata)

            guard case let .v1(.chunked(chunkedFile)) = envelope else {
                throw FileDownloadingError.invalidResumeMetadata
            }

            let state = DownloadEntry.ChunkedState(
                metadata: DownloadFileMetadata(
                    totalSize: chunkedFile.totalSize,
                    chunkHashes: chunkedFile.chunks
                ),
                lastChunkIndex: chunked.lastChunkIndex,
                downloadedBytes: chunked.downloadedBytes
            )

            return .chunked(state)
        }
    }

    func resumeInlineDownloading(
        downloadedBytes: Int,
        continuation: AsyncStream<FileDownloadingEvent>.Continuation
    ) {
        continuation.yield(.onProgress(.init(
            downloaded: downloadedBytes,
            total: downloadedBytes
        )))
    }

    func resumeChunkedDownloading(
        state: DownloadEntry.ChunkedState,
        claimer: FileClaimer,
        store: DownloadFileContextProtocol,
        continuation: AsyncStream<FileDownloadingEvent>.Continuation
    ) async throws {
        let totalSize = Int(state.metadata.totalSize)
        let totalChunks = state.metadata.chunkHashes.count
        let startIndex = state.lastChunkIndex.map { $0 + 1 } ?? 0

        var downloadedBytes = state.downloadedBytes

        if startIndex > 0 {
            continuation.yield(.onProgress(.init(
                downloaded: downloadedBytes,
                total: totalSize
            )))
        }

        for index in startIndex ..< totalChunks {
            try Task.checkCancellation()

            let chunkHash = state.metadata.chunkHashes[index]

            guard
                let encryptedChunk = try await service.claimData(
                    by: chunkHash,
                    recipient: claimer.proofProvider
                ) else {
                throw FileDownloadingError.noChunk(chunkHash)
            }

            let chunk = try claimer.decryptor.decrypt(encryptedChunk)
            try await store.appendChunk(chunk, at: index)

            try await service.acknowledgeReceivedData(
                by: chunkHash,
                recipient: claimer.proofProvider
            )

            downloadedBytes += chunk.count

            continuation.yield(.onProgress(.init(
                downloaded: downloadedBytes,
                total: totalSize
            )))
        }
    }
}
