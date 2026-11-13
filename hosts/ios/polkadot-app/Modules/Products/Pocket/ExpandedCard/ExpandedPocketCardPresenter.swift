import Foundation
import Products
import enum TrUAPIHost.ExpandedCardFaceOutcome
import UIKitExt

@MainActor
final class ExpandedPocketCardPresenter {
    weak var view: ExpandedPocketCardViewProtocol?
    let wireframe: ExpandedPocketCardWireframeProtocol
    let interactor: ExpandedPocketCardInteractorInputProtocol
    let hostProvider: ProductHostProviding

    init(
        interactor: ExpandedPocketCardInteractorInputProtocol,
        wireframe: ExpandedPocketCardWireframeProtocol,
        hostProvider: ProductHostProviding
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.hostProvider = hostProvider
    }
}

extension ExpandedPocketCardPresenter: ExpandedPocketCardPresenterProtocol {
    func setup(engine: JSEngineProtocol) {
        view?.showLoading()
        interactor.setup(engine: engine)
    }

    func didBecomeVisible() {
        interactor.checkForNewBuild()
    }

    func didTapRetry() {
        view?.showLoading()
        interactor.retry()
    }

    func didInterceptNavigation(to url: URL) {
        guard let productHost = hostProvider.host(url: url) else { return }

        wireframe.showProductSPA(productHost: productHost)
    }

    func didRequestDeeplink(_ url: URL) {
        wireframe.openDeeplink(url)
    }
}

extension ExpandedPocketCardPresenter: ExpandedPocketCardInteractorOutputProtocol {
    func didRequestNavigation(to url: URL) {
        view?.hideLoading()
        view?.navigate(to: url)
    }

    func didRequestReload() {
        view?.reload()
    }

    func didUpdateLoadPhase(_ phase: ProductLoadPhase) {
        view?.updateLoadPhase(phase)
    }

    func didFail(error: Error) {
        let content = wireframe.errorContent(from: error) ?? ErrorContent(
            title: String(localized: .Common.error),
            message: String(localized: .Common.errorMessage)
        )

        view?.showLoadFailure(content)
    }

    func didRequestFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        view?.setFaceShown(shown) ?? .notPresented
    }
}
