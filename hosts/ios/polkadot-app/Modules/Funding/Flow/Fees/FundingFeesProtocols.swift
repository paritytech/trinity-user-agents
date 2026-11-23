import Foundation
import UIKitExt

@MainActor
protocol FundingFeesViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: FundingFeesViewModel)
}

@MainActor
protocol FundingFeesPresenterProtocol: AnyObject {
    func setup()
    func back()
}

@MainActor
protocol FundingFeesInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol FundingFeesWireframeProtocol: AnyObject {
    func back()
}

extension FundingFlowWireframe: FundingFeesWireframeProtocol {}
