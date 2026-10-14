import Foundation
import HandoffService
import Operation_iOS
import Testing

@testable import polkadot_app

@Suite("UploadFileContext")
struct UploadFileContextTests {
    private let facade = UserDataStorageTestFacade()

    private func makeTempFile(data: Data) throws -> (url: URL, dir: URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)

        let url = dir.appendingPathComponent("upload-test.bin")
        try data.write(to: url)
        return (url, dir)
    }

    private func removeTempDir(_ dir: URL) {
        try? FileManager.default.removeItem(at: dir)
    }

    private func makeContext(
        fileData: Data,
        attachmentId: String = "msg1:file1",
        nodeProvider: HOPNodeProviding = MockHOPNodeProvider()
    ) throws -> (context: UploadFileContext, dir: URL) {
        let (fileURL, dir) = try makeTempFile(data: fileData)
        let factory = MixnetUploadRepositoryFactory(storageFacade: facade)

        let context = UploadFileContext(
            attachmentId: attachmentId,
            fileURL: fileURL,
            fileSize: fileData.count,
            nodeProvider: nodeProvider,
            repository: factory.createRepository(),
            updateRepository: factory.createUpdateRepository()
        )

        return (context, dir)
    }

    // MARK: - ensureUploadCredentials

    @Test("ensureUploadCredentials generates ticket and persists node")
    func ensureUploadCredentialsGenerates() async throws {
        let (context, dir) = try makeContext(fileData: Data(repeating: 0x01, count: 100))
        defer { removeTempDir(dir) }

        let credentials = try await context.ensureUploadCredentials()
        #expect(credentials.ticket.count == 32)
        #expect(credentials.node == MockHopNodes.trusted)
    }

    @Test("ensureUploadCredentials returns same credentials on repeated calls")
    func ensureUploadCredentialsIdempotent() async throws {
        let (context, dir) = try makeContext(
            fileData: Data(repeating: 0x01, count: 100),
            attachmentId: "msg2:file2"
        )
        defer { removeTempDir(dir) }

        let creds1 = try await context.ensureUploadCredentials()
        let creds2 = try await context.ensureUploadCredentials()

        #expect(creds1.ticket == creds2.ticket)
        #expect(creds1.node == creds2.node)
    }

    @Test("ensureUploadCredentials throws when no node available")
    func ensureUploadCredentialsThrowsNoNode() async throws {
        let (context, dir) = try makeContext(
            fileData: Data(repeating: 0x01, count: 100),
            attachmentId: "msg2b:file2b",
            nodeProvider: MockHOPNodeProvider(allowedNodes: [])
        )
        defer { removeTempDir(dir) }

        await #expect(throws: HOPFileLoaderError.self) {
            _ = try await context.ensureUploadCredentials()
        }
    }

    // MARK: - fetchResumeInfo

    @Test("fetchResumeInfo returns no progress for fresh upload")
    func freshResumeInfo() async throws {
        let (context, dir) = try makeContext(
            fileData: Data(repeating: 0x01, count: 100),
            attachmentId: "msg3:file3"
        )
        defer { removeTempDir(dir) }

        let info = try await context.fetchResumeInfo()
        #expect(info.fileSize == 100)
        #expect(info.progress == nil)
    }

    @Test("fetchResumeInfo returns progress after saving chunks")
    func resumeInfoWithProgress() async throws {
        let (context, dir) = try makeContext(
            fileData: Data(repeating: 0x01, count: 200),
            attachmentId: "msg4:file4"
        )
        defer { removeTempDir(dir) }

        _ = try await context.ensureUploadCredentials()

        let hash = try Data.randomOrError(of: 32)
        try await context.saveUploadedChunk(hash, uploadedSize: 100)

        let info = try await context.fetchResumeInfo()
        #expect(info.progress?.uploadedHashes == [hash])
        #expect(info.progress?.uploadedSize == 100)
    }

    // MARK: - fetchChunk

    @Test("fetchChunk reads correct slice from file")
    func fetchChunkReadsCorrectSlice() async throws {
        let originalChunk0 = Data(repeating: 0xAA, count: 100)
        let originalChunk1 = Data(repeating: 0xBB, count: 100)
        let originalChunk2 = Data(repeating: 0xCC, count: 50)

        var fileData = originalChunk0
        fileData.append(originalChunk1)
        fileData.append(originalChunk2)

        let (context, dir) = try makeContext(fileData: fileData, attachmentId: "msg5:file5")
        defer { removeTempDir(dir) }

        let chunk0 = try await context.fetchChunk(after: 0, size: 100)
        #expect(chunk0 == originalChunk0)

        let chunk1 = try await context.fetchChunk(after: 100, size: 100)
        #expect(chunk1 == originalChunk1)

        let chunk2 = try await context.fetchChunk(after: 200, size: 100)
        #expect(chunk2 == originalChunk2)
    }

    // MARK: - saveUploadedChunk

    @Test("saveUploadedChunk appends hashes incrementally")
    func saveChunkAppendsHashes() async throws {
        let (context, dir) = try makeContext(
            fileData: Data(repeating: 0x01, count: 300),
            attachmentId: "msg6:file6"
        )
        defer { removeTempDir(dir) }

        _ = try await context.ensureUploadCredentials()

        let h1 = Data(repeating: 0x01, count: 32)
        let h2 = Data(repeating: 0x02, count: 32)
        let h3 = Data(repeating: 0x03, count: 32)

        try await context.saveUploadedChunk(h1, uploadedSize: 100)
        try await context.saveUploadedChunk(h2, uploadedSize: 200)
        try await context.saveUploadedChunk(h3, uploadedSize: 300)

        let info = try await context.fetchResumeInfo()
        #expect(info.progress?.uploadedHashes == [h1, h2, h3])
        #expect(info.progress?.uploadedSize == 300)
    }

    // MARK: - finishUploading

    @Test("finishUploading true deletes DB record")
    func finishUploadingSuccess() async throws {
        let (context, dir) = try makeContext(
            fileData: Data(repeating: 0x01, count: 100),
            attachmentId: "msg7:file7"
        )
        defer { removeTempDir(dir) }

        _ = try await context.ensureUploadCredentials()
        try await context.saveUploadedChunk(Data(repeating: 0x01, count: 32), uploadedSize: 100)

        try await context.finishUploading(true)

        let info = try await context.fetchResumeInfo()
        #expect(info.progress == nil)
    }

    @Test("finishUploading false keeps DB record")
    func finishUploadingFailure() async throws {
        let (context, dir) = try makeContext(
            fileData: Data(repeating: 0x01, count: 100),
            attachmentId: "msg8:file8"
        )
        defer { removeTempDir(dir) }

        let creds = try await context.ensureUploadCredentials()

        try await context.finishUploading(false)

        let credsAfter = try await context.ensureUploadCredentials()
        #expect(credsAfter.ticket == creds.ticket)
        #expect(credsAfter.node == creds.node)
    }

    // MARK: - Ticket persistence across contexts

    @Test("ticket and node persist and survive across contexts")
    func credentialsSurviveAcrossContexts() async throws {
        let fileData = Data(repeating: 0x01, count: 100)
        let (fileURL, dir) = try makeTempFile(data: fileData)
        defer { removeTempDir(dir) }

        let factory = MixnetUploadRepositoryFactory(storageFacade: facade)

        // First context uses trusted node
        let context1 = UploadFileContext(
            attachmentId: "msg9:file9",
            fileURL: fileURL,
            fileSize: fileData.count,
            nodeProvider: MockHOPNodeProvider(allowedNodes: [MockHopNodes.trusted]),
            repository: factory.createRepository(),
            updateRepository: factory.createUpdateRepository()
        )

        let creds = try await context1.ensureUploadCredentials()
        try await context1.saveUploadedChunk(Data(repeating: 0x01, count: 32), uploadedSize: 50)

        // Simulate app restart — new context with different node provider
        // to prove the persisted node is used, not the new provider's
        let context2 = UploadFileContext(
            attachmentId: "msg9:file9",
            fileURL: fileURL,
            fileSize: fileData.count,
            nodeProvider: MockHOPNodeProvider(allowedNodes: [MockHopNodes.untrusted]),
            repository: factory.createRepository(),
            updateRepository: factory.createUpdateRepository()
        )

        let credsAfterRestart = try await context2.ensureUploadCredentials()
        #expect(credsAfterRestart.ticket == creds.ticket)
        #expect(credsAfterRestart.node == MockHopNodes.trusted)

        let info = try await context2.fetchResumeInfo()
        #expect(info.progress?.uploadedHashes.count == 1)
        #expect(info.progress?.uploadedSize == 50)
    }
}
