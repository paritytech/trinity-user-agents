import Foundation
@testable import polkadot_app

final class RecordingAdapterDelegate: TrUAPIChainRpcAdapterDelegate {
    private(set) var produced: [String] = []

    func adapter(_: TrUAPIChainRpcAdapter, didProduce json: String) {
        produced.append(json)
    }
}
