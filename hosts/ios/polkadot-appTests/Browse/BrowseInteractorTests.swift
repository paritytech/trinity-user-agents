import Foundation
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import UIKit
@testable import polkadot_app
@testable import Products

final class BrowseInteractorTests {
    private func makeHost() -> ProductHost? {
        ProductHost.parse("browse.dot", tld: "dot")
    }

    @Test
    @MainActor
    func syncHostReturnsWithoutAsyncCall() async throws {
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

        try await presenter.waitForOutcome()

        #expect(presenter.didResolveCallCount == 1)
        #expect(presenter.lastResolvedHost?.name == host.name)
        #expect(stub.resolveHostCallCount == 1)
    }

    @Test
    @MainActor
    func syncNilThenAsyncSuccessResolves() async throws {
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

        try await presenter.waitForOutcome()

        #expect(presenter.didResolveCallCount == 1)
        #expect(presenter.lastResolvedHost?.name == host.name)
    }

    @Test
    @MainActor
    func syncNilThenAsyncThrowFails() async throws {
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

        try await presenter.waitForOutcome()

        #expect(presenter.didFailResolveCallCount == 1)
        #expect(presenter.didResolveCallCount == 0)
    }

    @Test
    @MainActor
    func syncNilThenAsyncNilFails() async throws {
        let stub = StubProductHostProvider(
            syncHostResult: nil,
            asyncHostResult: .success(nil)
        )
        let presenter = StubPresenter()
        let logger = Logger.shared

        let interactor = BrowseInteractor(hostProvider: stub, logger: logger)
        interactor.presenter = presenter

        interactor.resolveBrowseHost()

        try await presenter.waitForOutcome()

        #expect(presenter.didFailResolveCallCount == 1)
        #expect(presenter.didResolveCallCount == 0)
    }

    @Test
    @MainActor
    func passesCorrectLabel() async throws {
        let stub = StubProductHostProvider(
            syncHostResult: nil,
            asyncHostResult: .success(nil)
        )
        let presenter = StubPresenter()
        let logger = Logger.shared

        let interactor = BrowseInteractor(hostProvider: stub, logger: logger)
        interactor.presenter = presenter

        interactor.resolveBrowseHost()

        try await presenter.waitForOutcome()

        #expect(stub.lastLabel == AppConfig.DotNs.dotNsBrowse)
    }
}

// MARK: - Stubs

@MainActor
private final class StubPresenter: BrowseInteractorOutputProtocol {
    var didResolveCallCount = 0
    var didFailResolveCallCount = 0
    var lastResolvedHost: ProductHost?

    private let outcomes: AsyncStream<Void>
    private let outcomeContinuation: AsyncStream<Void>.Continuation

    init() {
        (outcomes, outcomeContinuation) = AsyncStream<Void>.makeStream()
    }

    /// Returns once the interactor has reported an outcome, failing rather than hanging if it never does.
    func waitForOutcome() async throws {
        try await withTimeout(.hangGuard) { [outcomes] in
            for await _ in outcomes {
                return
            }
            throw CancellationError()
        }
    }

    func didResolve(host: ProductHost) {
        didResolveCallCount += 1
        lastResolvedHost = host
        outcomeContinuation.yield()
    }

    func didFailResolving() {
        didFailResolveCallCount += 1
        outcomeContinuation.yield()
    }
}
