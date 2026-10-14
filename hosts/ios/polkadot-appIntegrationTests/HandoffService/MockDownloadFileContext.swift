import Foundation
import HandoffService

final class MockDownloadFileContext: DownloadFileContextProtocol {
    let entryHash: FileHash

    private var metadataBlob: Data?
    private var fileData = Data()
    private var inlineData: Data?
    private var lastIndex: Int?
    private(set) var isFinished = false

    init(entryHash: FileHash) {
        self.entryHash = entryHash
    }

    func saveEntry(_ entry: DownloadedEntry) async throws {
        switch entry {
        case let .inline(fileData):
            inlineData = fileData
        case let .chunked(metadata, _):
            metadataBlob = metadata
        }
    }

    func fetchResumeInfo() async throws -> ResumeDownloadInfo? {
        if let inlineData {
            return .inline(downloadedBytes: inlineData.count)
        }

        guard let metadataBlob else { return nil }

        return .chunked(.init(
            metadata: metadataBlob,
            lastChunkIndex: lastIndex,
            downloadedBytes: fileData.count
        ))
    }

    func appendChunk(_ data: Data, at index: Int) async throws {
        fileData.append(data)
        lastIndex = index
    }

    func finishDownloading(_ fullFileDownloaded: Bool) async throws {
        isFinished = fullFileDownloaded
    }

    func assembleFile() -> Data {
        inlineData ?? fileData
    }
}
