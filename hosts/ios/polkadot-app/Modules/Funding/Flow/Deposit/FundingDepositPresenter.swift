import Foundation

/// Where to send the funds, as the provider reported it, over the session's
/// progress.
final class FundingDepositPresenter: FundingScreenPresenter {
    weak var view: FundingDepositViewProtocol?
    private let interactor: FundingDepositInteractorInputProtocol
    private let wireframe: FundingDepositWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol

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
        view?.didReceive(viewModel: viewModelFactory.makeDepositViewModel(for: state))
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
