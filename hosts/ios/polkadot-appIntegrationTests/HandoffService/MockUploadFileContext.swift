import Foundation
import HandoffService

final class MockUploadFileContext: UploadFileContextProtocol {
    private let fileData: Data
    private var uploadedHashes: [Data] = []
    private var totalUploaded: Int64 = 0

    init(fileData: Data) {
        self.fileData = fileData
    }

    func fetchResumeInfo() async throws -> ResumeUploadInfo {
        let progress: ResumeUploadInfo.Progress? =
            if !uploadedHashes.isEmpty {
                .init(uploadedHashes: uploadedHashes, uploadedSize: Int(totalUploaded))
            } else {
                nil
            }

        return ResumeUploadInfo(fileSize: fileData.count, progress: progress)
    }

    func fetchChunk(after bytesCount: Int, size: Int) async throws -> Data {
        let end = min(bytesCount + size, fileData.count)
        return fileData.subdata(in: bytesCount ..< end)
    }

    func saveUploadedChunk(_ hash: Data, uploadedSize: Int64) async throws {
        uploadedHashes.append(hash)
        totalUploaded = uploadedSize
    }

    func finishUploading(_: Bool) async throws {}
}
