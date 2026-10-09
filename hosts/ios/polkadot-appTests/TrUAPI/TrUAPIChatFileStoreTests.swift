import Foundation
import Testing
import UIKit
import TrUAPIHost
@testable import polkadot_app

private final class ChatFileFixture {
    let root: URL
    let sources: AttachmentStore
    let exports: AttachmentStore
    let store: TrUAPIChatFileStore

    init() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        sources = AttachmentStore(baseDirectory: root.appendingPathComponent("sources", isDirectory: true))
        exports = AttachmentStore(baseDirectory: root.appendingPathComponent("exports", isDirectory: true))
        store = TrUAPIChatFileStore(sources: sources, exports: exports)
    }

    deinit { try? FileManager.default.removeItem(at: root) }

    func original(_ data: Data) throws -> URL {
        let url = root.appendingPathComponent("selected.bin")
        try data.write(to: url)
        return url
    }
}

struct TrUAPIChatFileStoreTests {
    @Test func immutableSourceSurvivesOriginalMutationAndStoreRecreationUntilRelease() async throws {
        let fixture = try ChatFileFixture()
        let original = try fixture.original(Data([1, 2, 3, 4]))
        let picked = try await fixture.store.importFiles([original])
        let file = try #require(picked.first)
        #expect(file.metadata.sizeBytes == 4)
        #expect(file.metadata.kind == .file)
        #expect(file.metadata.mimeType == "application/octet-stream")

        try Data([9, 8]).write(to: original)
        try FileManager.default.removeItem(at: original)
        let reopened = TrUAPIChatFileStore(sources: fixture.sources, exports: fixture.exports)
        #expect(try await reopened.read(sourceId: file.sourceId, offset: 1, length: 3) == Data([2, 3, 4]))
        try await reopened.release(sourceId: file.sourceId)
        try await reopened.release(sourceId: file.sourceId)
        await #expect(throws: ChatFileFailure.self) {
            try await reopened.read(sourceId: file.sourceId, offset: 0, length: 1)
        }
    }

    @Test func readsCheckActualBoundsOverflowAndChunkLimit() async throws {
        let fixture = try ChatFileFixture()
        let original = try fixture.original(Data([1, 2, 3]))
        let picked = try await fixture.store.importFiles([original])
        let file = try #require(picked.first)
        #expect(try await fixture.store.read(sourceId: file.sourceId, offset: 3, length: 0) == Data())
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.read(sourceId: file.sourceId, offset: 2, length: 2)
        }
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.read(sourceId: file.sourceId, offset: UInt64.max, length: 1)
        }
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.read(sourceId: file.sourceId, offset: 0, length: 2_000_001)
        }
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.read(sourceId: "../selected.bin", offset: 0, length: 1)
        }
    }

    @Test func oversizedSourcesAndSymlinkHandlesAreRejected() async throws {
        let fixture = try ChatFileFixture()
        let original = try fixture.original(Data())
        let handle = try FileHandle(forWritingTo: original)
        try handle.truncate(atOffset: UInt64(UInt32.max) + 1)
        try handle.close()
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.importFiles([original])
        }
        try Data([1]).write(to: original)
        try fixture.sources.createDirectoryIfNeeded()
        let id = UUID().uuidString
        try FileManager.default.createSymbolicLink(at: fixture.sources.fileURL(for: id), withDestinationURL: original)
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.read(sourceId: id, offset: 0, length: 1)
        }
    }

    @Test func exportsRequireContiguousExactSizeAndCancelPreservesSavedCopy() async throws {
        let fixture = try ChatFileFixture()
        let id = try await fixture.store.beginExport(size: 4)
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.write(exportId: id, offset: 1, data: Data([1]))
        }
        try await fixture.store.write(exportId: id, offset: 0, data: Data([1, 2]))
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.write(exportId: id, offset: 0, data: Data([1, 2]))
        }
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.write(exportId: id, offset: 2, data: Data([3, 4, 5]))
        }
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.sealExport(exportId: id)
        }
        try await fixture.store.write(exportId: id, offset: 2, data: Data([3, 4]))
        let sealed = try await fixture.store.sealExport(exportId: id)
        #expect(try Data(contentsOf: sealed) == Data([1, 2, 3, 4]))
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.write(exportId: id, offset: 4, data: Data())
        }
        let savedCopy = fixture.root.appendingPathComponent("user-export.bin")
        try FileManager.default.copyItem(at: sealed, to: savedCopy)
        try await fixture.store.completeExport(exportId: id)
        try await fixture.store.cancelExport(exportId: id)
        try await fixture.store.cancelExport(exportId: id)
        #expect(try Data(contentsOf: savedCopy) == Data([1, 2, 3, 4]))
    }

    @Test func partialExportsCanBeCancelledAfterStoreRecreation() async throws {
        let fixture = try ChatFileFixture()
        let id = try await fixture.store.beginExport(size: 4)
        try await fixture.store.write(exportId: id, offset: 0, data: Data([1]))
        let reopened = TrUAPIChatFileStore(sources: fixture.sources, exports: fixture.exports)
        try await reopened.cancelExport(exportId: id)
        try await reopened.cancelExport(exportId: id)
        #expect(!fixture.exports.hasFile(for: id + ".bin"))
        await #expect(throws: ChatFileFailure.self) {
            try await reopened.write(exportId: id, offset: 1, data: Data([2]))
        }
    }

    @MainActor
    @Test func cancellingInFlightSaveDismissesOperationAndRemovesOnlyStaging() async throws {
        let fixture = try ChatFileFixture()
        let presenter = WaitingChatFilePresenter()
        let backend = TrUAPINativeChatFiles(store: fixture.store, presenter: presenter)
        let request = NativeChatFileExportRequest(
            productId: "chat.test",
            peerIdentity: Data(repeating: 1, count: 32),
            peerUsername: nil,
            metadata: HostNativeChatAttachmentMetadata(mimeType: "application/octet-stream", sizeBytes: 1, kind: .file)
        )
        let id = try #require(try await backend.beginChatFileExport(request: request))
        try await backend.writeChatFileExport(exportId: id, offset: 0, data: Data([7]))
        let finish = Task { try await backend.finishChatFileExport(exportId: id) }
        await presenter.waitUntilSaving()
        try await backend.cancelChatFileExport(exportId: id)
        await #expect(throws: CancellationError.self) { try await finish.value }
        #expect(!fixture.exports.hasFile(for: id + ".bin"))
        try await backend.cancelChatFileExport(exportId: id)
    }

    @Test func sdkWithoutBackendRejectsRatherThanReportingUserCancellationOrSuccess() async {
        let backend = UnavailableNativeChatFiles()
        let pick = NativeChatFilePickRequest(
            productId: "chat.test", peerIdentity: Data(repeating: 1, count: 32), peerUsername: nil, maxFiles: 1
        )
        let export = NativeChatFileExportRequest(
            productId: pick.productId, peerIdentity: pick.peerIdentity, peerUsername: nil,
            metadata: HostNativeChatAttachmentMetadata(mimeType: "application/octet-stream", sizeBytes: 1, kind: .file)
        )
        await #expect(throws: HostRejection.self) { try await backend.pickChatFiles(request: pick) }
        await #expect(throws: HostRejection.self) { try await backend.readChatFile(sourceId: "", offset: 0, length: 1) }
        await #expect(throws: HostRejection.self) { try await backend.releaseChatFile(sourceId: "") }
        await #expect(throws: HostRejection.self) { try await backend.beginChatFileExport(request: export) }
        await #expect(throws: HostRejection.self) { try await backend.writeChatFileExport(exportId: "", offset: 0, data: Data()) }
        await #expect(throws: HostRejection.self) { try await backend.finishChatFileExport(exportId: "") }
        await #expect(throws: HostRejection.self) { try await backend.cancelChatFileExport(exportId: "") }
    }

    @MainActor
    @Test func rasterMetadataUsesActualBytesNotExtensionAndBlurHashIsText() async throws {
        let fixture = try ChatFileFixture()
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let image = UIGraphicsImageRenderer(size: CGSize(width: 4, height: 3), format: format).image { context in
            UIColor.red.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 4, height: 3))
        }
        let png = try #require(image.pngData())
        let original = try fixture.original(png)
        let picked = try await fixture.store.importFiles([original])
        let file = try #require(picked.first)
        #expect(file.metadata.mimeType == "image/png")
        #expect(file.metadata.sizeBytes == UInt32(png.count))
        guard case let .image(width, height, thumbnail) = file.metadata.kind else {
            Issue.record("A real raster image must retain its native media metadata")
            return
        }
        #expect(width == 4)
        #expect(height == 3)
        let blurHash = try #require(thumbnail)
        #expect(String(data: blurHash, encoding: .utf8) != nil)
        #expect(try await fixture.store.read(sourceId: file.sourceId, offset: 0, length: UInt32(png.count)) == png)
        let export = try await fixture.store.beginExport(size: UInt32(png.count))
        try await fixture.store.write(exportId: export, offset: 0, data: png)
        let sealed = try await fixture.store.sealExport(exportId: export)
        #expect(sealed.pathExtension == "png")
        #expect(try Data(contentsOf: sealed) == png)
        try await fixture.store.completeExport(exportId: export)
        try await fixture.store.cancelExport(exportId: export)
        #expect(!FileManager.default.fileExists(atPath: sealed.path))
    }

    @Test func activeDocumentsStayOpaqueAndExportChunksAreBounded() async throws {
        let fixture = try ChatFileFixture()
        let svg = Data("<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>".utf8)
        let picked = try await fixture.store.importFiles([fixture.original(svg)])
        let file = try #require(picked.first)
        #expect(file.metadata.kind == .file)
        #expect(file.metadata.mimeType == "application/octet-stream")
        let export = try await fixture.store.beginExport(size: UInt32(svg.count))
        try await fixture.store.write(exportId: export, offset: 0, data: svg)
        let sealed = try await fixture.store.sealExport(exportId: export)
        #expect(sealed.pathExtension == "bin")
        #expect(try Data(contentsOf: sealed) == svg)
        try await fixture.store.completeExport(exportId: export)

        let oversized = try await fixture.store.beginExport(size: 2_000_001)
        await #expect(throws: ChatFileFailure.self) {
            try await fixture.store.write(exportId: oversized, offset: 0, data: Data(repeating: 0, count: 2_000_001))
        }
        try await fixture.store.cancelExport(exportId: oversized)
    }
}

@MainActor
private final class WaitingChatFilePresenter: TrUAPIChatFilePresenting {
    private var saving = false
    private var started: CheckedContinuation<Void, Never>?

    func selectFiles(request: NativeChatFilePickRequest) async throws -> [URL] { [] }
    func approveExport(request: NativeChatFileExportRequest) async throws -> Bool { true }

    func saveFile(_ url: URL, exportId: String) async throws -> Bool {
        saving = true
        started?.resume()
        started = nil
        try await Task.sleep(for: .seconds(30))
        return false
    }

    func waitUntilSaving() async {
        if saving { return }
        await withCheckedContinuation { started = $0 }
    }
}
