import Testing
import Foundation
import SubstrateSdk
import NovaCrypto
@testable import HandoffService

struct HandoffFileLoaderBlobTests {
    @Test func uploadBlobStoresVersionedInlineEntry() async throws {
        let (loader, service) = makeLoader()
        let blobData = Data([0xDE, 0xAD, 0xBE, 0xEF])

        let hash = try await loader.uploadBlob(
            blobData,
            sender: NoProofProvider(),
            recipients: makeRecipients()
        )

        // Wire vector: version index (v1) + payload index (inline) + compact length + bytes
        let expectedEntry = Data([0x00, 0x00, 0x10]) + blobData
        #expect(service.storage[hash] == expectedEntry)
    }

    @Test func uploadBlobRejectsOverInlineLimit() async throws {
        let (loader, _) = makeLoader()
        let oversized = try Data.randomOrError(of: inlineThreshold + 1)

        await #expect(throws: FileUploadingError.self) {
            try await loader.uploadBlob(
                oversized,
                sender: NoProofProvider(),
                recipients: makeRecipients()
            )
        }
    }

    @Test func downloadBlobConfirmsBeforeAck() async throws {
        let (loader, service) = makeLoader()
        let blobData = try Data.randomOrError(of: 32)
        let entryHash = try populateInlineEntry(service, with: blobData)

        var confirmedData: Data?
        var ackedBeforeConfirm = true

        try await loader.downloadBlob(entryHash, claimer: makeClaimer()) { data in
            confirmedData = data
            ackedBeforeConfirm = !service.ackedHashes.isEmpty
        }

        #expect(confirmedData == blobData)
        #expect(!ackedBeforeConfirm)
        #expect(service.ackedHashes == [entryHash])
    }

    @Test func downloadBlobRejectsChunkedPayload() async throws {
        let (loader, service) = makeLoader()
        let chunkData = try Data.randomOrError(of: 32)
        let chunkHash = try chunkData.blake2b32()
        service.storage[chunkHash] = chunkData

        let chunkedFile = ChunkedFile(totalSize: UInt64(chunkData.count), chunks: [chunkHash])
        let entry = try VersionedUploadedFile.v1(.chunked(chunkedFile)).scaleEncoded()
        let entryHash = try entry.blake2b32()
        service.storage[entryHash] = entry

        var confirmCalled = false

        await #expect(throws: FileDownloadingError.self) {
            try await loader.downloadBlob(entryHash, claimer: makeClaimer()) { _ in
                confirmCalled = true
            }
        }

        // The malformed entry is neither expanded nor acked, and no chunks are fetched
        #expect(!confirmCalled)
        #expect(service.ackedHashes.isEmpty)
        #expect(service.claimCallCount == 1)
    }

    @Test func downloadBlobThrowsWhenEntryMissing() async throws {
        let (loader, service) = makeLoader()
        let missingHash = try Data.randomOrError(of: 32)

        await #expect(throws: FileDownloadingError.self) {
            try await loader.downloadBlob(missingHash, claimer: makeClaimer()) { _ in }
        }

        #expect(service.ackedHashes.isEmpty)
    }

    @Test func downloadBlobFallsBackToRemoteStoreOnNotFound() async throws {
        let service = MockHandoffService()
        let remoteStore = MockLongTermRemoteStore()
        let decorator = HandoffServiceDecorator(handoffService: service, remoteStore: remoteStore)
        let loader = HandoffFileLoader(service: decorator, config: makeConfig())

        let blobData = try Data.randomOrError(of: 32)
        let entry = try VersionedUploadedFile.v1(.inline(blobData)).scaleEncoded()
        let entryHash = try entry.blake2b32()

        service.claimErrorsByHash[entryHash] = JSONRPCError(
            message: "Not found",
            code: HOPErrorCode.notFound,
            data: nil
        )
        remoteStore.storage[entryHash] = entry

        var confirmedData: Data?

        try await loader.downloadBlob(entryHash, claimer: makeClaimer()) { data in
            confirmedData = data
        }

        #expect(confirmedData == blobData)
        #expect(remoteStore.requestedHashes == [entryHash])
        // Chain-sourced entries are gone from the pool — the decorator skips the ack
        #expect(service.ackedHashes.isEmpty)
    }
}

// MARK: - Helpers

private extension HandoffFileLoaderBlobTests {
    var inlineThreshold: Int { makeConfig().inlineThreshold }

    func makeConfig() -> HandoffFileLoadConfig {
        HandoffFileLoadConfig(chunkSize: 100)
    }

    func makeLoader() -> (HandoffFileLoader, MockHandoffService) {
        let service = MockHandoffService()
        let loader = HandoffFileLoader(service: service, config: makeConfig())
        return (loader, service)
    }

    func makeRecipients() -> FileRecipients {
        FileRecipients(
            pubKeys: [.sr25519(Data(repeating: 1, count: 32))],
            encryptor: PassthroughEncryptor()
        )
    }

    func makeClaimer() -> FileClaimer {
        FileClaimer(
            proofProvider: MockRecipientProofProvider(),
            decryptor: PassthroughEncryptor()
        )
    }

    func populateInlineEntry(
        _ service: MockHandoffService,
        with blobData: Data
    ) throws -> Data {
        let entry = try VersionedUploadedFile.v1(.inline(blobData)).scaleEncoded()
        let entryHash = try entry.blake2b32()
        service.storage[entryHash] = entry

        return entryHash
    }
}
