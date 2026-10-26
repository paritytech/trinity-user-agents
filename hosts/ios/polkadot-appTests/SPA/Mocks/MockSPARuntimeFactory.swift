import Products
@testable import polkadot_app

/// Hands out one prepared runtime, so the interactor under test drives the runtime the test set up.
@MainActor
final class MockSPARuntimeFactory: SPARuntimeFactoryProtocol {
    private let runtime: SPARuntimeProtocol

    init(runtime: SPARuntimeProtocol) {
        self.runtime = runtime
    }

    func createRuntime(for _: ProductId) throws -> SPARuntimeProtocol {
        runtime
    }
}
