import Testing
import Foundation
import HandoffService
import SubstrateSdk
import NovaCrypto
import SDKLogger
@testable import polkadot_app

struct BitswapRemoteStoreTests {
    static let bulletInURL = URL(string: "wss://previewnet.substrate.dev/bulletin")!

    // Promotion to on-chain storage happens only near the 24h retention boundary and
    // can't be triggered from the client, so the live assertions cover the RPC surface:
    // the node exposes `bitswap_v1_get`, accepts our CID encoding (no InvalidCid), and
    // a missing entry maps to nil.
    // TODO: (Chat/HOP) Add a promoted-entry fetch test once previewnet provides a way to
    // force promotion or a stable promoted fixture hash.
    @Test func downloadReturnsNilForUnknownEntry() async throws {
        let store = try makeStore()

        let unknownHash = try Data.randomOrError(of: 32)

        let data = try await store.downloadData(by: unknownHash)

        #expect(data == nil)
    }
}

private extension BitswapRemoteStoreTests {
    func makeConnection() throws -> JSONRPCEngine {
        guard let connection = WebSocketEngine(urls: [Self.bulletInURL], logger: Logger.shared) else {
            throw TestSetupError.noConnection
        }

        return connection
    }

    func makeStore() throws -> BitswapRemoteStore {
        try BitswapRemoteStore(connection: makeConnection())
    }

    enum TestSetupError: Error {
        case noConnection
    }
}
