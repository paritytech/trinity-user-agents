import Foundation
import HandoffService
import Operation_iOS
import SDKLogger

actor UploadFileContext {
    let attachmentId: String
    let fileURL: URL
    let fileSize: Int
    let nodeProvider: HOPNodeProviding
    let repository: AnyDataProviderRepository<MixnetUpload>
    let updateRepository: AnyDataProviderRepository<MixnetUploadUpdate>
    let logger: LoggerProtocol

    private var fileHandle: FileHandle?

    init(
        attachmentId: String,
        fileURL: URL,
        fileSize: Int,
        nodeProvider: HOPNodeProviding,
        repository: AnyDataProviderRepository<MixnetUpload>,
        updateRepository: AnyDataProviderRepository<MixnetUploadUpdate>,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.attachmentId = attachmentId
        self.fileURL = fileURL
        self.fileSize = fileSize
        self.nodeProvider = nodeProvider
        self.repository = repository
        self.updateRepository = updateRepository
        self.logger = logger
    }
}

extension UploadFileContext {
    func ensureUploadCredentials() async throws -> MixnetUploadCredentials {
        let id = attachmentId

        let existing = try await repository.fetchOperation(
            by: { id },
            options: RepositoryFetchOptions()
        )
        .asyncExecute()

        if let ticket = existing?.ticket, let nodeURL = existing?.node {
            return MixnetUploadCredentials(
                ticket: ticket,
                node: .wssUrl(nodeURL)
            )
        }

        guard let node = nodeProvider.selectNode() else {
            throw HOPFileLoaderError.noAvailableNodes
        }

        let ticket = try FileTicket.generateFileTicket()

        let nodeURL: String =
            switch node {
            case let .wssUrl(url): url
            }

        let model = MixnetUpload(
            attachmentId: id,
            ticket: ticket,
            node: nodeURL,
            uploadedHashes: nil,
            uploadedSize: 0
        )

        try await repository.saveOperation({ [model] }, { [] }).asyncExecute()

        return MixnetUploadCredentials(ticket: ticket, node: node)
    }
}

extension UploadFileContext: UploadFileContextProtocol {
    func fetchResumeInfo() async throws -> ResumeUploadInfo {
        let id = attachmentId

        let optUpload = try await repository.fetchOperation(
            by: { id },
            options: RepositoryFetchOptions()
        )
        .asyncExecute()

        guard let upload = optUpload, let hashes = upload.uploadedHashes else {
            return ResumeUploadInfo(fileSize: fileSize, progress: nil)
        }

        logger.debug("Upload resume: \(hashes.count) chunks, \(upload.uploadedSize) bytes")

        return ResumeUploadInfo(
            fileSize: fileSize,
            progress: .init(
                uploadedHashes: hashes,
                uploadedSize: Int(upload.uploadedSize)
            )
        )
    }

    func fetchChunk(after bytesCount: Int, size: Int) throws -> Data {
        let handle = try getOrCreateHandle()

        try handle.seek(toOffset: UInt64(bytesCount))

        guard let data = try handle.read(upToCount: size), !data.isEmpty else {
            throw UploadFileContextError.noData
        }

        return data
    }

    func saveUploadedChunk(_ hash: Data, uploadedSize: Int64) async throws {
        let update = MixnetUploadUpdate(
            attachmentId: attachmentId,
            chunkHash: hash,
            uploadedSize: uploadedSize
        )

        try await updateRepository.saveOperation({ [update] }, { [] }).asyncExecute()

        logger.debug("Saved upload chunk, uploaded: \(uploadedSize)")
    }

    func finishUploading(_ fullFileUploaded: Bool) async throws {
        closeHandle()

        guard fullFileUploaded else {
            return
        }

        let id = attachmentId
        try await repository.saveOperation({ [] }, { [id] }).asyncExecute()
    }
}

private extension UploadFileContext {
    func getOrCreateHandle() throws -> FileHandle {
        if let fileHandle {
            return fileHandle
        }

        let handle = try FileHandle(forReadingFrom: fileURL)
        fileHandle = handle

        return handle
    }

    func closeHandle() {
        if let fileHandle {
            try? fileHandle.close()
            self.fileHandle = nil
        }
    }
}

enum UploadFileContextError: Error {
    case noData
}
