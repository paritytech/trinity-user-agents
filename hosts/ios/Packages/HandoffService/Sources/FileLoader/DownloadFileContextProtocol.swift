import Foundation

public struct DownloadFileMetadata: Equatable {
    public let totalSize: UInt64
    public let chunkHashes: [FileHash]

    public init(totalSize: UInt64, chunkHashes: [FileHash]) {
        self.totalSize = totalSize
        self.chunkHashes = chunkHashes
    }
}

public enum DownloadedEntry {
    case inline(fileData: Data)
    case chunked(metadata: Data, totalChunks: Int)
}

public enum ResumeDownloadInfo {
    public struct Chunked {
        public let metadata: Data
        public let lastChunkIndex: Int?
        public let downloadedBytes: Int

        public init(metadata: Data, lastChunkIndex: Int?, downloadedBytes: Int) {
            self.metadata = metadata
            self.lastChunkIndex = lastChunkIndex
            self.downloadedBytes = downloadedBytes
        }
    }

    case inline(downloadedBytes: Int)
    case chunked(Chunked)
}

public protocol DownloadFileContextProtocol {
    func saveEntry(_ entry: DownloadedEntry) async throws

    func fetchResumeInfo() async throws -> ResumeDownloadInfo?

    func appendChunk(_ data: Data, at index: Int) async throws

    func finishDownloading(_ fullFileDownloaded: Bool) async throws
}
