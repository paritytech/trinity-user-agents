import Foundation

@testable import polkadot_app

final class MockDecisionResolver: DecisionResolver {
    private let destination: RootDestination

    init(destination: RootDestination = .broken) {
        self.destination = destination
    }

    func resolve() throws -> RootDestination {
        destination
    }
}
