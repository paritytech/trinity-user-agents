import UIKit
import Foundation_iOS
import PolkadotUI
import ChainRegistry

final class DepositPresenter {
    weak var view: DepositViewProtocol?
    private let wireframe: DepositWireframeProtocol
    private let interactor: DepositInteractorInputProtocol
    private let viewModelFactory: DepositViewModelMaking
    private let assetIn: ChainAsset
    private let assetOut: ChainAsset

    private var depositSummary: DepositSummary?
    private var depositOperations: [DepositOperationModel] = []

    init(
        interactor: DepositInteractorInputProtocol,
        wireframe: DepositWireframeProtocol,
        assetIn: ChainAsset,
        assetOut: ChainAsset,
        viewModelFactory: DepositViewModelMaking
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.assetIn = assetIn
        self.assetOut = assetOut
        self.viewModelFactory = viewModelFactory
    }
}

private extension DepositPresenter {
    func provideAssetsViewModel() {
        let viewModel = viewModelFactory.makeAssetsViewModel(for: assetIn)

        view?.didReceive(assetsViewModel: viewModel)
    }

    func provideSummaryViewModel() {
        guard let depositSummary else {
            return
        }

        let viewModel = viewModelFactory.makeSummaryViewModel(
            for: depositSummary,
            assetIn: assetIn,
            assetOut: assetOut
        )

        view?.didReceive(summaryViewModel: viewModel)
    }

    func provideOperationsViewModel() {
        let operationsViewModel = viewModelFactory.makeOperationsViewModel(
            for: depositOperations,
            assetOut: assetOut
        )

        view?.didReceive(operationsViewModel: operationsViewModel)
    }

    func dismissViewModel() -> TitleDetailsSheetViewModel {
        let doneAction = MessageSheetAction(
            title: LocalizableResource { _ in
                String(localized: .Common.done)
            },
            handler: { [weak wireframe, weak view] in
                MainActor.assumeIsolated {
                    wireframe?.doneFunding(view: view)
                }
            }
        )

        return TitleDetailsSheetViewModel(
            graphics: nil,
            title: LocalizableResource { _ in
                String(localized: .depositDismissTitle)
            },
            message: LocalizableResource { _ in
                let string = String(localized: .depositDismissMessage)
                return .normal(string)
            },
            mainAction: doneAction,
            secondaryAction: .init(
                title: LocalizableResource { _ in
                    String(localized: .Common.goBack).capitalized
                },
                handler: {}
            )
        )
    }
}

extension DepositPresenter: DepositPresenterProtocol {
    func setup() {
        provideAssetsViewModel()

        interactor.setup()
    }

    func copyAddress() {
        guard let address = depositSummary?.depositAddress, let view else {
            return
        }

        wireframe.copyString(from: view, stringToCopy: address)
    }

    func done() {
        guard depositOperations.isEmpty else {
            wireframe.showDismissConfirmation(
                view: view,
                viewModel: dismissViewModel()
            )
            return
        }
        wireframe.close(view: view)
    }
}

extension DepositPresenter: DepositInteractorOutputProtocol {
    func didReceive(depositSummary: DepositSummary) {
        self.depositSummary = depositSummary
        provideSummaryViewModel()
    }

    func didReceive(operations: [DepositOperationModel]) {
        depositOperations = operations
        provideOperationsViewModel()
    }
}
