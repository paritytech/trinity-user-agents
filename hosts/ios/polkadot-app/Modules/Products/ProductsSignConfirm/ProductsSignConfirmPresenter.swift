import Foundation
import PolkadotUI

/// Drives the reused `PolkadotSigningViewController` for rust-core signing
/// confirmations: the sign action approves the request instead of signing.
@MainActor
final class ProductsSignConfirmPresenter {
    weak var view: PolkadotSigningViewProtocol?

    private let interactor: ProductsSignConfirmInteractorInputProtocol
    private let wireframe: ProductsSignConfirmWireframeProtocol

    private var model: ProductsSignConfirmModel?
    private var isInProgress = false
    private var failureMessage: String?

    init(
        interactor: ProductsSignConfirmInteractorInputProtocol,
        wireframe: ProductsSignConfirmWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension ProductsSignConfirmPresenter: PolkadotSigningPresenterProtocol {
    func setup() {
        interactor.setup()
        provideViewModel()
    }

    func sign() {
        guard model != nil else {
            return
        }
        wireframe.hide(view: view, decision: true)
    }

    func cancel() {
        wireframe.hide(view: view, decision: false)
    }

    func viewDetails() {
        guard let model else {
            return
        }
        wireframe.showViewDetails(
            with: model.detailsText,
            isTransaction: model.isTransaction,
            view: view
        )
    }
}

extension ProductsSignConfirmPresenter: ProductsSignConfirmInteractorOutputProtocol {
    func didStartParsingRequest() {
        isInProgress = true
        provideViewModel()
    }

    func didFinishParsingRequest(with model: ProductsSignConfirmModel) {
        self.model = model
        isInProgress = false
        provideViewModel()
    }

    func didFailToParseRequest(with error: Error) {
        isInProgress = false
        failureMessage = failureText(for: error)
        provideViewModel()
    }
}

private extension ProductsSignConfirmPresenter {
    func provideViewModel() {
        if let failureMessage {
            view?.didReceive(viewModel: .failure(failureMessage))
        } else if isInProgress {
            view?.didReceive(viewModel: .inProgress)
        } else if let model {
            view?.didReceive(viewModel: .result(.init(
                hostName: model.requester.name,
                iconViewModel: model.requester.iconUrl.map {
                    RemoteImageViewModel(url: $0)
                },
                transactionDescription: model.descriptionText
            )))
        }
    }

    /// A localized failure title plus the specific mapping error, so a failed
    /// review shows what went wrong rather than a generic message.
    func failureText(for error: Error) -> String {
        let detail = (error as? LocalizedError)?.errorDescription ?? String(describing: error)
        return String(localized: .polkadotSigningFailure) + "\n" + detail
    }
}
