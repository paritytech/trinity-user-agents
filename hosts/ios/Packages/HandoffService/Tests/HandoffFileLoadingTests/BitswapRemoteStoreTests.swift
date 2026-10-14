import Testing
import Foundation
import SubstrateSdk
import NovaCrypto
@testable import HandoffService

struct BitswapRemoteStoreTests {
    @Test func requestUsesSpecCompliantCid() async throws {
        // CIDv1, raw codec 0x55, multihash blake2b-256 (0xb220) over the entry hash,
        // rendered as multibase base32-lower. Pinned against an independent encoder.
        let fileHash = Data(1 ... 32)
        let expectedCid = "bafk2bzaceaaqeayeaudaocajbifqydiob4ibceqtcqkrmfyydenbwha5dypsa"

        let engine = MockJSONRPCEngine()
        engine.result = .failure(JSONRPCError(message: "not found", code: -32_810, data: nil))
        let store = BitswapRemoteStore(connection: engine)

        _ = try await store.downloadData(by: fileHash)

        #expect(engine.lastMethod == "bitswap_v1_get")
        #expect(engine.lastParams == [expectedCid])
    }

    @Test func downloadReturnsVerifiedData() async throws {
        let data = try Data.randomOrError(of: 100)
        let fileHash = try data.blake2b32()

        let engine = MockJSONRPCEngine()
        engine.result = .success(data.toHex())
        let store = BitswapRemoteStore(connection: engine)

        let downloaded = try await store.downloadData(by: fileHash)

        #expect(downloaded == data)
    }

    @Test func downloadReturnsNilOnHashMismatch() async throws {
        let data = try Data.randomOrError(of: 100)
        let fileHash = try Data.randomOrError(of: 32)

        let engine = MockJSONRPCEngine()
        engine.result = .success(data.toHex())
        let store = BitswapRemoteStore(connection: engine)

        let downloaded = try await store.downloadData(by: fileHash)

        #expect(downloaded == nil)
    }

    @Test func downloadReturnsNilOnNotFound() async throws {
        let engine = MockJSONRPCEngine()
        engine.result = .failure(JSONRPCError(message: "not found", code: -32_810, data: nil))
        let store = BitswapRemoteStore(connection: engine)

        let downloaded = try await store.downloadData(by: Data(1 ... 32))

        #expect(downloaded == nil)
    }

    @Test func downloadRethrowsOtherErrors() async throws {
        let engine = MockJSONRPCEngine()
        engine.result = .failure(JSONRPCError(message: "major syncing", code: -32_812, data: nil))
        let store = BitswapRemoteStore(connection: engine)

        await #expect(throws: JSONRPCError.self) {
            try await store.downloadData(by: Data(1 ... 32))
        }
    }
}
