import UIKit

/// Where to send the funds, as the provider reported it, over the session's
/// progress.
final class FundingDepositPresenter: FundingScreenPresenter {
    weak var view: FundingDepositViewProtocol?
    private let interactor: FundingDepositInteractorInputProtocol
    private let wireframe: FundingDepositWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol
    /// The QR code drawn last, with the payload it carries.
    private var qrCode: (payload: String, image: UIImage)?
    private var requestedQRPayload: String?

    init(
        context: FundingFlowContext,
        interactor: FundingDepositInteractor,
        wireframe: FundingDepositWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        super.init(context: context, interactor: interactor)
    }

    override func render() {
        let viewModel = viewModelFactory.makeDepositViewModel(for: state)
        guard case let .crypto(deposit) = viewModel.content else {
            view?.didReceive(viewModel: viewModel, qrCode: nil)
            return
        }

        if requestedQRPayload != deposit.payload {
            requestedQRPayload = deposit.payload
            interactor.createQRCode(for: deposit.payload)
        }
        let image = qrCode.flatMap { $0.payload == deposit.payload ? $0.image : nil }
        view?.didReceive(viewModel: viewModel, qrCode: image)
    }
}

extension FundingDepositPresenter: FundingDepositInteractorOutputProtocol {
    func didReceive(qrCode image: UIImage, for payload: String) {
        qrCode = (payload, image)
        render()
    }
}

extension FundingDepositPresenter: FundingDepositPresenterProtocol {
    func back() {
        guard !entry.hasStarted else {
            wireframe.showProgress()
            return
        }

        interactor.stopStarting()
        wireframe.back()
    }

    func showProgress() {
        wireframe.showProgress()
    }

    func confirmCancel() {
        wireframe.show(.cancelConfirm)
    }

    func didCopy() {
        wireframe.showCopied()
    }

    func close() {
        wireframe.close(then: nil)
    }
}
