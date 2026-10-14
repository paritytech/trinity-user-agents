import Foundation

public struct ResumeUploadInfo {
    public struct Progress {
        public let uploadedHashes: [Data]
        public let uploadedSize: Int

        public init(uploadedHashes: [Data], uploadedSize: Int) {
            self.uploadedHashes = uploadedHashes
            self.uploadedSize = uploadedSize
        }
    }

    public let fileSize: Int
    public let progress: Progress?

    public init(fileSize: Int, progress: Progress?) {
        self.fileSize = fileSize
        self.progress = progress
    }
}

public protocol UploadFileContextProtocol {
    func fetchResumeInfo() async throws -> ResumeUploadInfo
    func fetchChunk(after bytesCount: Int, size: Int) async throws -> Data
    func saveUploadedChunk(_ hash: Data, uploadedSize: Int64) async throws
    func finishUploading(_ fullFileUploaded: Bool) async throws
}
