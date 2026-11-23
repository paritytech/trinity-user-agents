import Foundation
import UIKitExt

@MainActor
protocol FundingTokenViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: FundingTokenViewModel)
}

@MainActor
protocol FundingTokenPresenterProtocol: AnyObject {
    func setup()
    func chooseToken(_ symbol: String)
    func back()
}

@MainActor
protocol FundingTokenInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol FundingTokenWireframeProtocol: AnyObject {
    func show(_ screen: FundingFlowScreen)
    func back()
}

extension FundingFlowWireframe: FundingTokenWireframeProtocol {}
