import UIKit
import UIKitExt
import PolkadotUI

protocol DepositViewProtocol: ControllerBackedProtocol {
    func didReceive(assetsViewModel: DepositAssetsViewModel)
    func didReceive(summaryViewModel: DepositSummaryViewModel)
    func didReceive(operationsViewModel: [DepositOperationViewModel])
}

@MainActor
protocol DepositPresenterProtocol: AnyObject {
    func setup()
    func copyAddress()
    func done()
}

protocol DepositInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol DepositInteractorOutputProtocol: AnyObject {
    func didReceive(depositSummary: DepositSummary)
    func didReceive(operations: [DepositOperationModel])
}

@MainActor
protocol DepositWireframeProtocol: CommonCopyPresentable {
    func close(view: DepositViewProtocol?)
    func doneFunding(view: DepositViewProtocol?)

    func showDismissConfirmation(
        view: ControllerBackedProtocol?,
        viewModel: TitleDetailsSheetViewModel
    )
}
