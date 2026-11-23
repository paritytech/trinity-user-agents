import Foundation
import UIKitExt
import TrUAPIHost

@MainActor
protocol FundingProvidersViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: FundingProvidersViewModel)
}

@MainActor
protocol FundingProvidersPresenterProtocol: AnyObject {
    func setup()
    func chooseProvider(_ providerId: ProductId)
    func back()
}

@MainActor
protocol FundingProvidersInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol FundingProvidersWireframeProtocol: AnyObject {
    func back()
    func showToast(_ message: String)
}

extension FundingFlowWireframe: FundingProvidersWireframeProtocol {}
