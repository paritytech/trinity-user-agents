import Foundation
import AsyncExtensions

public typealias HandoffBlobConfirmClosure = (Data) async throws -> Void

public protocol HandoffFileLoading {
    func uploadFile(
        store: UploadFileContextProtocol,
        sender: SenderProofProviding,
        recipients: FileRecipients
    ) -> AnyAsyncSequence<FileUploadingEvent>

    func downloadFile(
        using entryHash: FileHash,
        claimer: FileClaimer,
        store: DownloadFileContextProtocol
    ) -> AnyAsyncSequence<FileDownloadingEvent>

    func uploadBlob(
        _ data: Data,
        sender: SenderProofProviding,
        recipients: FileRecipients
    ) async throws -> FileHash

    func downloadBlob(
        _ hash: FileHash,
        claimer: FileClaimer,
        onConfirm: @escaping HandoffBlobConfirmClosure
    ) async throws
}
