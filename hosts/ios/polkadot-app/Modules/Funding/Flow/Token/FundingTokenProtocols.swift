import Foundation

typealias FundingTokenViewProtocol = FundingScreenViewProtocol

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
