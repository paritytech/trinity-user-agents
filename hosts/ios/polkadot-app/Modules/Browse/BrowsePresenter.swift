import Foundation
import Products
import UIKit

@MainActor
final class BrowsePresenter: BrowsePresenterProtocol {
    weak var view: BrowseViewProtocol?
    let interactor: BrowseInteractorInputProtocol
    let wireframe: BrowseWireframeProtocol

    private var resolvedHost: ProductHost?
    private var isOnScreen = false
    private var isShown = false
    private var pendingShow: Task<Void, Never>?

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

    func didAppear() {
        isOnScreen = true
        scheduleShowIfReady()
    }

    func didDisappear() {
        isOnScreen = false
        pendingShow?.cancel()
        pendingShow = nil
    }
}

// MARK: - BrowseInteractorOutputProtocol

extension BrowsePresenter: BrowseInteractorOutputProtocol {
    func didResolve(host: ProductHost) {
        resolvedHost = host
        scheduleShowIfReady()
    }

    func didFailResolving() {
        view?.hideLoading()
        presentRetry()
    }
}

// MARK: - Private

private extension BrowsePresenter {
    /// Defers the SPA build to a later main-actor turn, so the event that made it ready finishes first.
    func scheduleShowIfReady() {
        guard !isShown, isOnScreen, pendingShow == nil, let host = resolvedHost else { return }

        pendingShow = Task { @MainActor [weak self] in
            guard !Task.isCancelled else { return }
            self?.showSPA(host: host)
        }
    }

    func showSPA(host: ProductHost) {
        pendingShow = nil
        view?.hideLoading()

        guard wireframe.showSPA(from: view, host: host) else {
            presentRetry()
            return
        }
        isShown = true
    }

    func presentRetry() {
        wireframe.presentRequestStatus(on: view) { [weak self] in
            Task { @MainActor in
                self?.view?.showLoading()
                self?.interactor.resolveBrowseHost()
            }
        }
    }
}
