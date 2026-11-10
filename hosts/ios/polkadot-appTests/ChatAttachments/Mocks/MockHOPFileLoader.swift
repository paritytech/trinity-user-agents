import Foundation
import HandoffService
import AsyncExtensions

@testable import polkadot_app

final class MockHOPFileLoaderFactory: HOPFileLoaderMaking, @unchecked Sendable {
    let loader: HandoffFileLoading?
    let makeError: Error?
    private(set) var lastRequestedNode: ChatRemoteMessageContent.NodeEndpoint?

    init(loader: HandoffFileLoading? = nil, makeError: Error? = nil) {
        self.loader = loader
        self.makeError = makeError
    }

    func makeLoader(for node: ChatRemoteMessageContent.NodeEndpoint) throws -> HandoffFileLoading {
        lastRequestedNode = node
        if let makeError { throw makeError }
        guard let loader else { return MockHOPFileLoader() }
        return loader
    }
}

final class MockHOPFileLoader: HandoffFileLoading, @unchecked Sendable {
    var downloadEvents: [FileDownloadingEvent] = []
    var uploadEvents: [FileUploadingEvent] = []
    var uploadEventsPerAttempt: [[FileUploadingEvent]]?
    private(set) var uploadAttemptCount = 0
    var shouldSuspend = false
    var suspensionTime: Int = 60 // seconds

    func uploadFile(
        store _: UploadFileContextProtocol,
        sender _: SenderProofProviding,
        recipients _: FileRecipients
    ) -> AnyAsyncSequence<FileUploadingEvent> {
        uploadAttemptCount += 1

        // Attempts past the last script replay it.
        let events: [FileUploadingEvent] =
            if let scripts = uploadEventsPerAttempt {
                scripts[min(uploadAttemptCount - 1, scripts.count - 1)]
            } else {
                uploadEvents
            }

        let suspend = shouldSuspend
        let currentSuspensionTime = suspensionTime

        return AsyncStream { continuation in
            Task {
                for event in events {
                    continuation.yield(event)
                }

                if suspend {
                    try? await Task.sleep(for: .seconds(currentSuspensionTime))
                }

                continuation.finish()
            }
        }
        .eraseToAnyAsyncSequence()
    }

    func downloadFile(
        using _: FileHash,
        claimer _: FileClaimer,
        store _: DownloadFileContextProtocol
    ) -> AnyAsyncSequence<FileDownloadingEvent> {
        let events = downloadEvents
        let suspend = shouldSuspend
        let currentSuspensionTime = suspensionTime

        return AsyncStream { continuation in
            Task {
                for event in events {
                    continuation.yield(event)
                }

                if suspend {
                    try? await Task.sleep(for: .seconds(currentSuspensionTime))
                }

                continuation.finish()
            }
        }
        .eraseToAnyAsyncSequence()
    }

    func uploadBlob(
        _ data: Data,
        sender _: SenderProofProviding,
        recipients _: FileRecipients
    ) async throws -> FileHash {
        try data.blake2b32()
    }

    func downloadBlob(
        _: FileHash,
        claimer _: FileClaimer,
        onConfirm _: @escaping HandoffBlobConfirmClosure
    ) async throws {}
}
