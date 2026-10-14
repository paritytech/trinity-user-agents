import Testing
import Foundation
import UIKit
@testable import polkadot_app
@testable import Products

final class BrowseInteractorTests {
    private func makeHost() -> ProductHost? {
        ProductHost.parse("browse.dot", tld: "dot")
    }

    @Test
    @MainActor
    func syncHostReturnsWithoutAsyncCall() async {
        guard let host = makeHost() else {
            Issue.record("Failed to create ProductHost")
            return
        }

        let stub = StubProductHostProvider(
            syncHostResult: nil,
            asyncHostResult: .success(host)
        )
        let presenter = StubPresenter()
        let logger = Logger.shared

        let interactor = BrowseInteractor(hostProvider: stub, logger: logger)
        interactor.presenter = presenter

        interactor.resolveBrowseHost()

        // Let main actor hop complete
        try? await Task.sleep(for: .milliseconds(10))

        #expect(presenter.didResolveCallCount == 1)
        #expect(presenter.lastResolvedHost?.name == host.name)
        #expect(stub.resolveHostCallCount == 1)
    }

    @Test
    @MainActor
    func syncNilThenAsyncSuccessResolves() async {
        guard let host = makeHost() else {
            Issue.record("Failed to create ProductHost")
            return
        }

        let stub = StubProductHostProvider(
            syncHostResult: nil,
            asyncHostResult: .success(host)
        )
        let presenter = StubPresenter()
        let logger = Logger.shared

        let interactor = BrowseInteractor(hostProvider: stub, logger: logger)
        interactor.presenter = presenter

        interactor.resolveBrowseHost()

        // Let async operation and main actor hop complete
        try? await Task.sleep(for: .milliseconds(100))

        #expect(presenter.didResolveCallCount == 1)
        #expect(presenter.lastResolvedHost?.name == host.name)
    }

    @Test
    @MainActor
    func syncNilThenAsyncThrowFails() async {
        struct TestError: Error {}

        let stub = StubProductHostProvider(
            syncHostResult: nil,
            asyncHostResult: .failure(TestError())
        )
        let presenter = StubPresenter()
        let logger = Logger.shared

        let interactor = BrowseInteractor(hostProvider: stub, logger: logger)
        interactor.presenter = presenter

        interactor.resolveBrowseHost()

        // Let async operation and main actor hop complete
        try? await Task.sleep(for: .milliseconds(100))

        #expect(presenter.didFailResolveCallCount == 1)
        #expect(presenter.didResolveCallCount == 0)
    }

    @Test
    @MainActor
    func syncNilThenAsyncNilFails() async {
        let stub = StubProductHostProvider(
            syncHostResult: nil,
            asyncHostResult: .success(nil)
        )
        let presenter = StubPresenter()
        let logger = Logger.shared

        let interactor = BrowseInteractor(hostProvider: stub, logger: logger)
        interactor.presenter = presenter

        interactor.resolveBrowseHost()

        // Let async operation and main actor hop complete
        try? await Task.sleep(for: .milliseconds(100))

        #expect(presenter.didFailResolveCallCount == 1)
        #expect(presenter.didResolveCallCount == 0)
    }

    @Test
    @MainActor
    func passesCorrectLabel() async {
        let stub = StubProductHostProvider(
            syncHostResult: nil,
            asyncHostResult: .success(nil)
        )
        let presenter = StubPresenter()
        let logger = Logger.shared

        let interactor = BrowseInteractor(hostProvider: stub, logger: logger)
        interactor.presenter = presenter

        interactor.resolveBrowseHost()

        // Let async operation and main actor hop complete
        try? await Task.sleep(for: .milliseconds(100))

        #expect(stub.lastLabel == AppConfig.DotNs.dotNsBrowse)
    }
}

// MARK: - Stubs

@MainActor
private final class StubPresenter: BrowseInteractorOutputProtocol {
    var didResolveCallCount = 0
    var didFailResolveCallCount = 0
    var lastResolvedHost: ProductHost?

    func didResolve(host: ProductHost) {
        didResolveCallCount += 1
        lastResolvedHost = host
    }

    func didFailResolving() {
        didFailResolveCallCount += 1
    }
}
