import Foundation

@testable import polkadot_app

final class MockDecisionResolver: DecisionResolver {
    func resolve() throws -> RootDestination {
        .broken
    }
}
