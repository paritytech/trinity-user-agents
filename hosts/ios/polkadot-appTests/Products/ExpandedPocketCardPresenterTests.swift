import Foundation
import Products
import Testing
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

@MainActor
struct ExpandedPocketCardPresenterTests {
    @Test func setupShowsLoadingBeforeStartingThePage() {
        let environment = Environment()

        environment.presenter.setup(engine: MockJSEngine())

        #expect(environment.view.events == [.showLoading])
        #expect(environment.interactor.setupCount == 1)
    }

    @Test func servedPageHidesLoadingAndLoads() throws {
        let environment = Environment()
        let url = try #require(URL(string: "polkadotapp-product://test.dot/"))

        environment.presenter.didRequestNavigation(to: url)

        #expect(environment.view.events == [.hideLoading, .navigate(url)])
    }

    @Test func retryShowsLoadingAgain() {
        let environment = Environment()

        environment.presenter.didTapRetry()

        #expect(environment.view.events == [.showLoading])
        #expect(environment.interactor.retryCount == 1)
    }

    @Test func unknownFailureShowsTheGenericError() {
        let environment = Environment()

        environment.presenter.didFail(error: CancellationError())

        #expect(environment.view.events == [.showLoadFailure(title: String(localized: .Common.error))])
    }

    @Test func faceRequestsAnswerWithTheViewsOutcome() {
        let environment = Environment()
        environment.view.faceOutcome = .notPresented

        #expect(environment.presenter.didRequestFaceShown(true) == .notPresented)
        #expect(environment.view.faceRequests == [true])
    }
}

@MainActor
private struct Environment {
    let view = StubExpandedPocketCardView()
    let interactor = RecordingInteractor()
    let presenter: ExpandedPocketCardPresenter

    init() {
        presenter = ExpandedPocketCardPresenter(
            interactor: interactor,
            wireframe: ExpandedPocketCardWireframe(),
            hostProvider: ProductHostFactory(tldProvider: StubDotNsTldProvider(tld: "dot"))
        )
        presenter.view = view
    }
}

@MainActor
private final class RecordingInteractor: ExpandedPocketCardInteractorInputProtocol {
    private(set) var setupCount = 0
    private(set) var retryCount = 0

    func setup(engine _: JSEngineProtocol) { setupCount += 1 }
    func retry() { retryCount += 1 }
    func checkForNewBuild() {}
}
