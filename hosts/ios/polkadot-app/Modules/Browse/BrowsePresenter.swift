import Foundation
import Products
import UIKit

@MainActor
final class BrowsePresenter: BrowsePresenterProtocol {
    weak var view: BrowseViewProtocol?
    let interactor: BrowseInteractorInputProtocol
    let wireframe: BrowseWireframeProtocol

    init(
        interactor: BrowseInteractorInputProtocol,
        wireframe: BrowseWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }

    func setup() {
        view?.showLoading()
        interactor.resolveBrowseHost()
    }
}

// MARK: - BrowseInteractorOutputProtocol

extension BrowsePresenter: BrowseInteractorOutputProtocol {
    func didResolve(host: ProductHost) {
        view?.hideLoading()

        guard wireframe.showSPA(from: view, host: host) else {
            presentRetry()
            return
        }
    }

    func didFailResolving() {
        view?.hideLoading()
        presentRetry()
    }
}

// MARK: - Private

private extension BrowsePresenter {
    func presentRetry() {
        wireframe.presentRequestStatus(on: view) { [weak self] in
            Task { @MainActor in
                self?.view?.showLoading()
                self?.interactor.resolveBrowseHost()
            }
        }
    }
}
