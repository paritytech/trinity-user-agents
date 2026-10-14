import Foundation
import Products
import UIKit
import UIKitExt

final class SPAPresenter {
    weak var view: SPAViewProtocol?
    let interactor: SPAInteractorInputProtocol
    let wireframe: SPAWireframeProtocol
    let configuration: SPAConfiguration
    let hostProvider: ProductHostProviding

    init(
        interactor: SPAInteractorInputProtocol,
        wireframe: SPAWireframeProtocol,
        configuration: SPAConfiguration,
        hostProvider: ProductHostProviding
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.configuration = configuration
        self.hostProvider = hostProvider
    }
}

extension SPAPresenter: SPAPresenterProtocol {
    func setup(engine: JSEngineProtocol) {
        view?.showLoading()
        interactor.setup(engine: engine)
    }

    func didTapMoreButton() {
        var actions: [SPAMoreAction] = []

        if hasChatEntry() {
            actions.append(
                SPAMoreAction(
                    icon: .iconChatBubble,
                    title: String(localized: .spaActionOpenChat),
                    isEnabled: true,
                    handler: { [weak self] in self?.didTapOpenChat() }
                )
            )
        }

        [
            SPAMoreAction(
                icon: .iconRefresh,
                title: String(localized: .spaActionRefresh),
                isEnabled: true,
                handler: { [weak self] in self?.view?.reload() }
            ),
            SPAMoreAction(
                icon: .iconShare,
                title: String(localized: .spaActionShare),
                isEnabled: true,
                handler: { [weak self] in self?.didTapShare() }
            )
        ].forEach { actions.append($0) }

        wireframe.showMoreActions(
            from: view,
            actions: actions,
            closeTitle: String(localized: .spaActionClose)
        )
    }

    func didTapMinimize() {
        wireframe.minimize()
    }

    func didTapClose() {
        guard let browserTabId = configuration.browserTabId else { return }

        wireframe.close(tabId: browserTabId)
    }

    func didTapRetry() {
        view?.showLoading()
        interactor.retry()
    }

    func hasChatEntry() -> Bool {
        interactor.hasChatEntry()
    }

    func didTapOpenChat() {
        interactor.openChat()
    }

    func didTapShare() {
        let host = configuration.page.host.name
        guard let url = AppConfig.ProductUniversalLink.url(for: host) else {
            return
        }
        wireframe.shareURL(url, from: view)
    }

    func didInterceptNavigation(to url: URL) {
        guard let productHost = hostProvider.host(url: url) else { return }

        wireframe.showProductSPA(from: view, productHost: productHost)
    }

    func didUpdateWebViewTitle(_ title: String) {
        view?.updateTitle(title)
    }
}

extension SPAPresenter: SPAInteractorOutputProtocol {
    func didRequestNavigation(to url: URL) {
        view?.hideLoading()
        view?.navigate(to: url)
    }

    func didPrepareChat(chatId: Chat.Id) {
        wireframe.openChat(from: view, chatId: chatId)
    }

    func didUpdateLoadProgress(_ progress: DotNsLoadProgress) {
        view?.updateLoadProgress(progress)
    }

    func didFail(error: Error) {
        let content = wireframe.errorContent(from: error) ?? ErrorContent(
            title: String(localized: .Common.error),
            message: String(localized: .Common.errorMessage)
        )

        view?.showLoadFailure(content)
    }
}
