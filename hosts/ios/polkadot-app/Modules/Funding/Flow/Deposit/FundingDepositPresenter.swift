import Foundation

/// Where to send the funds, as the provider reported it. Crypto value in
/// starts here, on the core's best quote; once started, the progress screen
/// sits under it.
final class FundingDepositPresenter: FundingScreenPresenter {
    private let interactor: FundingDepositInteractorInputProtocol
    private let wireframe: FundingDepositWireframeProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingDepositInteractor,
        wireframe: FundingDepositWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        super.init(context: context, interactor: interactor)
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

    func close() {
        wireframe.close(then: nil)
    }
}
