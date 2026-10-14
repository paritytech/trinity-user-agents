import Foundation
import HandoffService
import Operation_iOS
import Testing

@testable import polkadot_app

@Suite("DownloadFileContext")
struct DownloadFileContextTests {
    private let facade = UserDataStorageTestFacade()

    private func makeTempStore() throws -> (store: AttachmentStore, dir: URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        return (AttachmentStore(fileManager: .default, baseDirectory: dir), dir)
    }

    private func removeTempDir(_ dir: URL) {
        try? FileManager.default.removeItem(at: dir)
    }

    private func makeContext(
        entryHash: Data = Data(repeating: 0xAA, count: 32),
        filename: String = "test-file.mp4",
        store: AttachmentStore? = nil,
        tempDir: URL? = nil
    ) throws -> (context: DownloadFileContext, dir: URL) {
        let (attachmentsStore, dir): (AttachmentStore, URL)

        if let store, let tempDir {
            (attachmentsStore, dir) = (store, tempDir)
        } else {
            (attachmentsStore, dir) = try makeTempStore()
        }

        let factory = MixnetDownloadRepositoryFactory(storageFacade: facade)

        let context = DownloadFileContext(
            entryHash: entryHash,
            filename: filename,
            attachmentsStore: attachmentsStore,
            repository: factory.createRepository(),
            chunkIndexRepository: factory.createChunkIndexRepository()
        )

        return (context, dir)
    }

    private func chunkedInfo(_ info: ResumeDownloadInfo?) -> ResumeDownloadInfo.Chunked? {
        if case let .chunked(chunked) = info { chunked } else { nil }
    }

    private func inlineBytes(_ info: ResumeDownloadInfo?) -> Int? {
        if case let .inline(downloadedBytes) = info { downloadedBytes } else { nil }
    }

    // MARK: - saveEntry

    @Test("saveEntry chunked persists to DB and fetchResumeInfo returns it")
    func saveAndFetchChunkedEntry() async throws {
        let (context, dir) = try makeContext()
        defer { removeTempDir(dir) }

        let metadata = Data("scale-encoded-metadata".utf8)

        try await context.saveEntry(.chunked(metadata: metadata, totalChunks: 5))

        let info = try #require(try await chunkedInfo(context.fetchResumeInfo()))
        #expect(info.metadata == metadata)
        #expect(info.lastChunkIndex == nil)
        #expect(info.downloadedBytes == 0)
    }

    @Test("saveEntry inline writes partial file and DB record in one call")
    func saveAndFetchInlineEntry() async throws {
        let (store, dir) = try makeTempStore()
        defer { removeTempDir(dir) }

        let (context, _) = try makeContext(filename: "photo.jpg", store: store, tempDir: dir)

        let fileData = Data(repeating: 0xEE, count: 120)
        try await context.saveEntry(.inline(fileData: fileData))

        #expect(store.hasFile(for: "photo.jpg.part"))

        let info = try await context.fetchResumeInfo()
        #expect(inlineBytes(info) == 120)
    }

    // MARK: - fetchResumeInfo

    @Test("fetchResumeInfo returns nil when no record exists")
    func fetchResumeInfoNil() async throws {
        let (context, dir) = try makeContext()
        defer { removeTempDir(dir) }

        let info = try await context.fetchResumeInfo()
        #expect(info == nil)
    }

    @Test("fetchResumeInfo drops inline record when file is shorter than DB expects")
    func inlineMismatchRestartsFresh() async throws {
        let (store, dir) = try makeTempStore()
        defer { removeTempDir(dir) }

        let entryHash = Data(repeating: 0xCC, count: 32)
        let (context, _) = try makeContext(entryHash: entryHash, store: store, tempDir: dir)

        // DB row claims 120 bytes but no partial file exists (interrupted write)
        let factory = MixnetDownloadRepositoryFactory(storageFacade: facade)
        let repo: AnyDataProviderRepository<MixnetDownload> = factory.createRepository()
        let model = MixnetDownload(
            entryHashHex: entryHash.toHex(),
            entryType: .inline,
            lastChunkIndex: -1,
            totalChunks: 0,
            metadata: nil,
            downloadedBytes: 120
        )
        try await repo.saveOperation({ [model] }, { [] }).asyncExecute()

        let info = try await context.fetchResumeInfo()
        #expect(info == nil)

        // The stale record is removed so the next attempt starts fresh
        let remaining = try await repo.fetchOperation(
            by: { entryHash.toHex() },
            options: .init()
        ).asyncExecute()
        #expect(remaining == nil)
    }

    // MARK: - appendChunk

    @Test("appendChunk writes data to partial file and updates DB")
    func appendChunkWritesAndPersists() async throws {
        let (context, dir) = try makeContext()
        defer { removeTempDir(dir) }

        try await context.saveEntry(.chunked(metadata: Data("meta".utf8), totalChunks: 2))

        let chunk0 = Data(repeating: 0x01, count: 100)
        try await context.appendChunk(chunk0, at: 0)

        let info = try #require(try await chunkedInfo(context.fetchResumeInfo()))
        #expect(info.lastChunkIndex == 0)
        #expect(info.downloadedBytes == 100)
    }

    @Test("appendChunk appends multiple chunks sequentially")
    func appendMultipleChunks() async throws {
        let (context, dir) = try makeContext()
        defer { removeTempDir(dir) }

        try await context.saveEntry(.chunked(metadata: Data("m".utf8), totalChunks: 3))

        try await context.appendChunk(Data(repeating: 0xAA, count: 50), at: 0)
        try await context.appendChunk(Data(repeating: 0xBB, count: 60), at: 1)
        try await context.appendChunk(Data(repeating: 0xCC, count: 40), at: 2)

        let info = try #require(try await chunkedInfo(context.fetchResumeInfo()))
        #expect(info.lastChunkIndex == 2)
        #expect(info.downloadedBytes == 150)
    }

    // MARK: - DB-before-file ordering

    @Test("downloadedBytes in DB matches expected total after chunk")
    func downloadedBytesMatchesExpected() async throws {
        let (context, dir) = try makeContext()
        defer { removeTempDir(dir) }

        try await context.saveEntry(.chunked(metadata: Data("m".utf8), totalChunks: 2))

        try await context.appendChunk(Data(repeating: 0x01, count: 100), at: 0)
        try await context.appendChunk(Data(repeating: 0x02, count: 80), at: 1)

        let info = try #require(try await chunkedInfo(context.fetchResumeInfo()))
        #expect(info.downloadedBytes == 180)
        #expect(info.lastChunkIndex == 1)
    }

    // MARK: - finishDownloading

    @Test("finishDownloading true moves file and deletes DB record")
    func finishDownloadingSuccess() async throws {
        let (store, dir) = try makeTempStore()
        defer { removeTempDir(dir) }

        let (context, _) = try makeContext(filename: "video.mp4", store: store, tempDir: dir)

        try await context.saveEntry(.chunked(metadata: Data("m".utf8), totalChunks: 1))
        try await context.appendChunk(Data(repeating: 0xFF, count: 50), at: 0)
        try await context.finishDownloading(true)

        #expect(store.hasFile(for: "video.mp4"))
        #expect(!store.hasFile(for: "video.mp4.part"))

        let info = try await context.fetchResumeInfo()
        #expect(info == nil)
    }

    @Test("finishDownloading true moves inline file and deletes DB record")
    func finishInlineDownloadingSuccess() async throws {
        let (store, dir) = try makeTempStore()
        defer { removeTempDir(dir) }

        let (context, _) = try makeContext(filename: "photo.jpg", store: store, tempDir: dir)

        let fileData = Data(repeating: 0x5A, count: 64)
        try await context.saveEntry(.inline(fileData: fileData))
        try await context.finishDownloading(true)

        #expect(store.hasFile(for: "photo.jpg"))
        #expect(!store.hasFile(for: "photo.jpg.part"))

        let savedData = try Data(contentsOf: store.fileURL(for: "photo.jpg"))
        #expect(savedData == fileData)

        let info = try await context.fetchResumeInfo()
        #expect(info == nil)
    }

    @Test("finishDownloading false keeps partial file and DB record")
    func finishDownloadingFailure() async throws {
        let (store, dir) = try makeTempStore()
        defer { removeTempDir(dir) }

        let (context, _) = try makeContext(filename: "video.mp4", store: store, tempDir: dir)

        try await context.saveEntry(.chunked(metadata: Data("m".utf8), totalChunks: 2))
        try await context.appendChunk(Data(repeating: 0x01, count: 50), at: 0)
        try await context.finishDownloading(false)

        #expect(store.hasFile(for: "video.mp4.part"))
        #expect(!store.hasFile(for: "video.mp4"))

        let info = try await context.fetchResumeInfo()
        #expect(info != nil)
    }

    // MARK: - Resume mismatch detection

    @Test("fetchResumeInfo rolls back chunk when DB ahead of file")
    func resumeMismatchRollsBack() async throws {
        let (store, dir) = try makeTempStore()
        defer { removeTempDir(dir) }

        let entryHash = Data(repeating: 0xBB, count: 32)

        let (context1, _) = try makeContext(entryHash: entryHash, store: store, tempDir: dir)
        try await context1.saveEntry(.chunked(metadata: Data("m".utf8), totalChunks: 3))

        try await context1.appendChunk(Data(repeating: 0x01, count: 100), at: 0)
        try await context1.appendChunk(Data(repeating: 0x02, count: 100), at: 1)

        // Simulate: chunk 2 was saved to DB but file write didn't complete
        let factory = MixnetDownloadRepositoryFactory(storageFacade: facade)
        let chunkRepo = factory.createChunkIndexRepository()
        let update = MixnetDownloadChunkIndex(
            entryHashHex: entryHash.toHex(),
            lastChunkIndex: 2,
            downloadedBytes: 300
        )
        try await chunkRepo.saveOperation({ [update] }, { [] }).asyncExecute()

        // New context on "relaunch" — file has 200 bytes but DB says 300
        let (context2, _) = try makeContext(entryHash: entryHash, store: store, tempDir: dir)
        let info = try #require(try await chunkedInfo(context2.fetchResumeInfo()))

        #expect(info.lastChunkIndex == 1)
        #expect(info.downloadedBytes == 200)
    }

    @Test("fetchResumeInfo rolls back to nil when first chunk DB ahead of file")
    func resumeMismatchFirstChunk() async throws {
        let (store, dir) = try makeTempStore()
        defer { removeTempDir(dir) }

        let entryHash = Data(repeating: 0xDD, count: 32)

        let (context, _) = try makeContext(entryHash: entryHash, store: store, tempDir: dir)
        try await context.saveEntry(.chunked(metadata: Data("m".utf8), totalChunks: 2))

        // DB says chunk 0 written with 100 bytes, but file is empty (0 bytes)
        let factory = MixnetDownloadRepositoryFactory(storageFacade: facade)
        let chunkRepo = factory.createChunkIndexRepository()
        let update = MixnetDownloadChunkIndex(
            entryHashHex: entryHash.toHex(),
            lastChunkIndex: 0,
            downloadedBytes: 100
        )
        try await chunkRepo.saveOperation({ [update] }, { [] }).asyncExecute()

        let info = try #require(try await chunkedInfo(context.fetchResumeInfo()))
        #expect(info.lastChunkIndex == nil)
        #expect(info.downloadedBytes == 0)
    }
}
