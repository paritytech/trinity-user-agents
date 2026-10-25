import Foundation
import TrUAPIHost

/// The same backend is provided to the runtime and every product execution, so
/// durable sources remain readable after execution teardown or app restart.
actor TrUAPINativeChatFiles: NativeChatFilesHost {
    static let shared = TrUAPINativeChatFiles()
    private let store: TrUAPIChatFileStore
    private let presenter: TrUAPIChatFilePresenting
    private var finishing: [String: Task<Void, Error>] = [:]

    init(
        store: TrUAPIChatFileStore = .shared,
        presenter: TrUAPIChatFilePresenting = TrUAPIChatFilePresenter()
    ) {
        self.store = store
        self.presenter = presenter
    }

    func pickChatFiles(request: NativeChatFilePickRequest) async throws -> [NativeChatPickedFile] {
        guard request.maxFiles > 0 else { throw ChatFileFailure.invalidRange }
        let urls = try await presenter.selectFiles(request: request)
        try Task.checkCancellation()
        guard urls.count <= Int(request.maxFiles) else { throw ChatFileFailure.invalidRange }
        return try await store.importFiles(urls)
    }

    func readChatFile(sourceId: String, offset: UInt64, length: UInt32) async throws -> Data {
        try await store.read(sourceId: sourceId, offset: offset, length: length)
    }

    func releaseChatFile(sourceId: String) async throws {
        try await store.release(sourceId: sourceId)
    }

    func beginChatFileExport(request: NativeChatFileExportRequest) async throws -> String? {
        guard try await presenter.approveExport(request: request) else { return nil }
        try Task.checkCancellation()
        return try await store.beginExport(size: request.metadata.sizeBytes)
    }

    func writeChatFileExport(exportId: String, offset: UInt64, data: Data) async throws {
        try await store.write(exportId: exportId, offset: offset, data: data)
    }

    func finishChatFileExport(exportId: String) async throws {
        guard finishing[exportId] == nil else { throw ChatFileFailure.invalidHandle }
        let operation = Task { [store, presenter] in
            do {
                let url = try await store.sealExport(exportId: exportId)
                guard try await presenter.saveFile(url, exportId: exportId) else {
                    throw ChatFileFailure.userCancelled
                }
                try await store.completeExport(exportId: exportId)
            } catch {
                try? await store.completeExport(exportId: exportId)
                throw error
            }
        }
        finishing[exportId] = operation
        defer { finishing.removeValue(forKey: exportId) }
        try await withTaskCancellationHandler {
            try await operation.value
        } onCancel: {
            operation.cancel()
        }
    }

    func cancelChatFileExport(exportId: String) async throws {
        if let operation = finishing[exportId] {
            // Also covers cancellation between sealing and presenting the UI.
            operation.cancel()
            _ = await operation.result
        }
        try await store.cancelExport(exportId: exportId)
    }
}
