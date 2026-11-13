import Foundation

@testable import polkadot_app

final class UnreachableFlowStateProvider: SPAFlowStateProviding {
    func flowState() -> SPAFlowState {
        preconditionFailure("Flow state must not be needed on this path")
    }
}
