import Foundation
import TrUAPIHost

typealias FundingCancelConfirmViewProtocol = FundingScreenViewProtocol

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
protocol FundingCancelConfirmWireframeProtocol: AnyObject {
    func back()
    func close(then completion: (() -> Void)?)
    func showToast(_ message: String)
}

extension FundingFlowWireframe: FundingCancelConfirmWireframeProtocol {}
