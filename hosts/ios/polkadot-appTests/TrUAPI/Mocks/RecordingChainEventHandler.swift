import Foundation
@testable import polkadot_app

final class RecordingChainEventHandler: TrUAPIChainEventHandling {
    private(set) var responses: [(UInt32, String)] = []
    private(set) var closed: [UInt32] = []

    func chainDidReceiveResponse(connectionId: UInt32, json: String) {
        responses.append((connectionId, json))
    }

    func chainDidClose(connectionId: UInt32) {
        closed.append(connectionId)
    }
}
