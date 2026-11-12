import Testing
import UIKit

@testable import polkadot_app

@Suite("Presented page dismissal")
@MainActor
struct PresentedPageDismissalTests {
    private final class MockPresenter: ModalPresenting {
        var presentedViewController: UIViewController?
        private(set) var dismissals: [Bool] = []

        func dismiss(animated flag: Bool, completion: (() -> Void)?) {
            dismissals.append(flag)
            presentedViewController = nil
            completion?()
        }
    }

    @Test("navigates straight away when nothing is presented")
    func nothingPresented() {
        let presenter = MockPresenter()
        var didNavigate = false

        PresentedPageDismissal.dismissingPresented(on: presenter) { didNavigate = true }

        #expect(didNavigate)
        #expect(presenter.dismissals.isEmpty)
    }

    @Test("dismisses the presented page without animation, then navigates")
    func presentedPage() {
        let presenter = MockPresenter()
        presenter.presentedViewController = UIViewController()
        var events: [String] = []

        PresentedPageDismissal.dismissingPresented(on: presenter) {
            events.append("navigated")
            #expect(presenter.presentedViewController == nil)
        }

        #expect(presenter.dismissals == [false])
        #expect(events == ["navigated"])
    }

    @Test("leaves a pinned presentation in place and still navigates")
    func pinnedPresentation() {
        let presenter = MockPresenter()
        let pinned = UIViewController()
        presenter.presentedViewController = pinned
        var didNavigate = false

        PresentedPageDismissal.dismissingPresented(
            on: presenter,
            unless: { $0 === pinned },
            then: { didNavigate = true }
        )

        #expect(didNavigate)
        #expect(presenter.dismissals.isEmpty)
        #expect(presenter.presentedViewController === pinned)
    }

    @Test("navigates when there is no presenter")
    func noPresenter() {
        var didNavigate = false

        PresentedPageDismissal.dismissingPresented(on: nil) { didNavigate = true }

        #expect(didNavigate)
    }
}
