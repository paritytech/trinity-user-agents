import Testing
import Foundation
import UIKit
import UIKitExt
@testable import polkadot_app
@testable import Products

@MainActor
final class BrowsePresenterTests {
    private func makeHost() -> ProductHost? {
        ProductHost.parse("browse.dot", tld: "dot")
    }

    @Test
    func setupShowsLoadingAndResolvesHost() {
        let view = StubView()
        let interactor = StubInteractor()
        let wireframe = StubWireframe()

        let presenter = BrowsePresenter(interactor: interactor, wireframe: wireframe)
        presenter.view = view

        presenter.setup()

        #expect(view.showLoadingCallCount == 1)
        #expect(interactor.resolveBrowseHostCallCount == 1)
    }

    @Test
    func didResolveHidesLoadingAndShowsSPA() {
        guard let host = makeHost() else {
            Issue.record("Failed to create ProductHost")
            return
        }

        let view = StubView()
        let interactor = StubInteractor()
        let wireframe = StubWireframe(showSPAResult: true)

        let presenter = BrowsePresenter(interactor: interactor, wireframe: wireframe)
        presenter.view = view

        presenter.didResolve(host: host)

        #expect(view.hideLoadingCallCount == 1)
        #expect(wireframe.showSPACallCount == 1)
        #expect(wireframe.lastHost?.name == host.name)
        #expect(wireframe.presentRetryCallCount == 0)
    }

    @Test
    func didResolveShowsRetryWhenSPAFails() {
        guard let host = makeHost() else {
            Issue.record("Failed to create ProductHost")
            return
        }

        let view = StubView()
        let interactor = StubInteractor()
        let wireframe = StubWireframe(showSPAResult: false)

        let presenter = BrowsePresenter(interactor: interactor, wireframe: wireframe)
        presenter.view = view

        presenter.didResolve(host: host)

        #expect(view.hideLoadingCallCount == 1)
        #expect(wireframe.presentRetryCallCount == 1)
    }

    @Test
    func didFailResolvingHidesLoadingAndShowsRetry() {
        let view = StubView()
        let interactor = StubInteractor()
        let wireframe = StubWireframe()

        let presenter = BrowsePresenter(interactor: interactor, wireframe: wireframe)
        presenter.view = view

        presenter.didFailResolving()

        #expect(view.hideLoadingCallCount == 1)
        #expect(wireframe.presentRetryCallCount == 1)
    }

    @Test
    func retryClosure() {
        let view = StubView()
        let interactor = StubInteractor()
        var retryClosureCaptured: (() -> Void)?
        let wireframe = StubWireframe(onPresentRetry: { closure in
            retryClosureCaptured = closure
        })

        let presenter = BrowsePresenter(interactor: interactor, wireframe: wireframe)
        presenter.view = view

        presenter.didFailResolving()

        guard let retryClosureCaptured else {
            Issue.record("Retry closure not captured")
            return
        }

        #expect(view.showLoadingCallCount == 0)
        #expect(interactor.resolveBrowseHostCallCount == 0)

        // The closure enqueues a Task which executes asynchronously; calling it
        // here only verifies the captured closure runs without crashing.
        retryClosureCaptured()
    }
}

// MARK: - Stubs

@MainActor
private final class StubView: BrowseViewProtocol {
    var showLoadingCallCount = 0
    var hideLoadingCallCount = 0

    var controller: UIViewController {
        UIViewController()
    }

    var isSetup: Bool {
        true
    }

    func showLoading() {
        showLoadingCallCount += 1
    }

    func hideLoading() {
        hideLoadingCallCount += 1
    }
}

private final class StubInteractor: BrowseInteractorInputProtocol {
    var resolveBrowseHostCallCount = 0

    func resolveBrowseHost() {
        resolveBrowseHostCallCount += 1
    }
}

@MainActor
private final class StubWireframe: BrowseWireframeProtocol {
    let showSPAResult: Bool
    var onPresentRetry: ((@escaping () -> Void) -> Void)?

    var showSPACallCount = 0
    var presentRetryCallCount = 0
    var lastHost: ProductHost?

    init(
        showSPAResult: Bool = true,
        onPresentRetry: ((@escaping () -> Void) -> Void)? = nil
    ) {
        self.showSPAResult = showSPAResult
        self.onPresentRetry = onPresentRetry
    }

    func showSPA(from _: BrowseViewProtocol?, host: ProductHost) -> Bool {
        showSPACallCount += 1
        lastHost = host
        return showSPAResult
    }

    func presentRequestStatus(
        on _: ControllerBackedProtocol?,
        title _: String = "Error",
        message _: String = "Please try again",
        cancelAction _: String = "Skip",
        locale _: Locale? = nil,
        retryAction: @escaping () -> Void
    ) {
        presentRetryCallCount += 1
        onPresentRetry?(retryAction)
    }

    func presentTryAgainOperation(
        on _: ControllerBackedProtocol?,
        title _: String = "Error",
        message _: String = "Please try again",
        actionTitle _: String = "Retry",
        retryAction _: @escaping () -> Void
    ) {}

    func present(
        message _: String?,
        title _: String?,
        closeAction _: String?,
        from _: ControllerBackedProtocol?
    ) {}

    func present(
        viewModel _: AlertPresentableViewModel,
        style _: UIAlertController.Style,
        from _: ControllerBackedProtocol?
    ) {}
}
