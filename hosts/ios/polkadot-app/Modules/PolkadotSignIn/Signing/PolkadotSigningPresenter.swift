import Foundation
import PolkadotUI

@MainActor
final class PolkadotSigningPresenter {
    weak var view: PolkadotSigningViewProtocol?

    private let interactor: PolkadotSigningInteractorInputProtocol
    private let wireframe: PolkadotSigningWireframeProtocol

    private var parsedResult: PolkadotParsedSigningRequestResult?
    private var isInProgress = false
    private var isFailed = false

    init(
        interactor: PolkadotSigningInteractorInputProtocol,
        wireframe: PolkadotSigningWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension PolkadotSigningPresenter: PolkadotSigningPresenterProtocol {
    func setup() {
        interactor.setup()
        provideViewModel()
    }

    func sign() {
        guard let parsedResult else {
            return
        }
        interactor.signParsedResult(parsedResult)
    }

    func cancel() {
        wireframe.hide(view: view, decision: .rejected)
    }

    func viewDetails() {
        guard let parsedResult else {
            return
        }
        wireframe.showViewDetails(
            with: parsedResult.detailsText,
            isTransaction: parsedResult.isTransaction,
            view: view
        )
    }
}

extension PolkadotSigningPresenter: PolkadotSigningInteractorOutputProtocol {
    func didStartParsingRequest() {
        markAsInProgress()
    }

    func didFinishParsingRequest(with result: PolkadotParsedSigningRequestResult) {
        parsedResult = result
        isInProgress = false
        provideViewModel()
    }

    func didFailToParseRequest(with _: Error) {
        markAsFailed()
    }

    func didStartSigning() {
        markAsInProgress()
    }

    func didFinishSigning(with result: PolkadotHostSigningResult) {
        wireframe.hide(view: view, decision: .signed(result))
    }

    func didFailToSign(with _: Error) {
        markAsFailed()
    }
}

private extension PolkadotSigningPresenter {
    func markAsInProgress() {
        isInProgress = true
        provideViewModel()
    }

    func markAsFailed() {
        isInProgress = false
        isFailed = true
        provideViewModel()
    }

    func provideViewModel() {
        if isFailed {
            view?.didReceive(viewModel: .failure(
                .init(localized: .polkadotSigningFailure)
            ))
        } else if isInProgress {
            view?.didReceive(viewModel: .inProgress)
        } else if let parsedResult {
            view?.didReceive(viewModel: .result(.init(
                hostName: parsedResult.requester.name,
                iconViewModel: parsedResult.requester.iconUrl.map {
                    RemoteImageViewModel(url: $0)
                },
                transactionDescription: parsedResult.parsedRequest.descriptionText
            )))
        }
    }
}
