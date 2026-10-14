import Foundation
import Operation_iOS
import Testing

@testable import polkadot_app

extension CoreDataMapperTests {
    @Suite("MixnetUploadMapper")
    struct MixnetUploadMapperTests {
        private let facade = UserDataStorageTestFacade()

        private var repo: AnyDataProviderRepository<MixnetUpload> {
            facade.makeRepo(mapper: MixnetUploadMapper())
        }

        private var updateRepo: AnyDataProviderRepository<MixnetUploadUpdate> {
            facade.makeRepo(mapper: MixnetUploadUpdateMapper())
        }

        @Test("roundTrip preserves all fields")
        func roundTrip() async throws {
            let ticket = Data(repeating: 0xAB, count: 32)
            let hashes: [Data] = [
                Data(repeating: 0x01, count: 32),
                Data(repeating: 0x02, count: 32)
            ]

            let original = MixnetUpload(
                attachmentId: "msg1:file1",
                ticket: ticket,
                node: "wss://test.node",
                uploadedHashes: hashes,
                uploadedSize: 2_048
            )

            try await repo.saveOperation({ [original] }, { [] }).asyncExecute()

            let result = try #require(
                try await repo.fetchOperation(
                    by: { original.identifier },
                    options: .init()
                ).asyncExecute()
            )

            #expect(result.attachmentId == original.attachmentId)
            #expect(result.ticket == ticket)
            #expect(result.node == "wss://test.node")
            #expect(result.uploadedHashes == hashes)
            #expect(result.uploadedSize == original.uploadedSize)
        }

        @Test("update mapper appends chunk hash to existing entity")
        func updateAppendsChunk() async throws {
            let original = MixnetUpload(
                attachmentId: "msg3:file3",
                ticket: Data(repeating: 0xCC, count: 32),
                node: "wss://test.node",
                uploadedHashes: nil,
                uploadedSize: 0
            )

            try await repo.saveOperation({ [original] }, { [] }).asyncExecute()

            let hash1 = Data(repeating: 0x01, count: 32)
            let update1 = MixnetUploadUpdate(
                attachmentId: "msg3:file3",
                chunkHash: hash1,
                uploadedSize: 100
            )

            try await updateRepo.saveOperation({ [update1] }, { [] }).asyncExecute()

            let after1 = try #require(
                try await repo.fetchOperation(
                    by: { original.identifier },
                    options: .init()
                ).asyncExecute()
            )

            #expect(after1.uploadedHashes == [hash1])
            #expect(after1.uploadedSize == 100)
            #expect(after1.ticket == original.ticket)

            let hash2 = Data(repeating: 0x02, count: 32)
            let update2 = MixnetUploadUpdate(
                attachmentId: "msg3:file3",
                chunkHash: hash2,
                uploadedSize: 200
            )

            try await updateRepo.saveOperation({ [update2] }, { [] }).asyncExecute()

            let after2 = try #require(
                try await repo.fetchOperation(
                    by: { original.identifier },
                    options: .init()
                ).asyncExecute()
            )

            #expect(after2.uploadedHashes == [hash1, hash2])
            #expect(after2.uploadedSize == 200)
        }
    }
}
