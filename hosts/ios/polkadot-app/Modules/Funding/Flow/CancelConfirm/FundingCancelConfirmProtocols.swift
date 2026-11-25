import Foundation
import UIKitExt
import TrUAPIHost

@MainActor
protocol FundingCancelConfirmViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: FundingCancelConfirmViewModel)
}

@MainActor
protocol FundingCancelConfirmPresenterProtocol: AnyObject {
    func setup()
    func requestCancel()
    func back()
}

@MainActor
protocol FundingCancelConfirmInteractorInputProtocol: AnyObject {
    func setup()
    func cancel() async throws -> CancelOutcome
}

@MainActor
protocol FundingCancelConfirmWireframeProtocol: FundingToastPresentable {
    func back()
    func close(then completion: (() -> Void)?)
}

extension FundingFlowWireframe: FundingCancelConfirmWireframeProtocol {}
