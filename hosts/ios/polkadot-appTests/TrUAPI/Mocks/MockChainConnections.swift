import Foundation
@testable import polkadot_app

final class MockChainConnections: TrUAPIChainConnecting, @unchecked Sendable {
    var eventHandler: TrUAPIChainEventHandling?

    private(set) var closeAllCallCount = 0

    func connect(genesisHash _: Data) -> UInt32? { nil }
    func send(connectionId _: UInt32, request _: String) throws {}
    func close(connectionId _: UInt32) {}

    func closeAll() {
        closeAllCallCount += 1
    }
}
