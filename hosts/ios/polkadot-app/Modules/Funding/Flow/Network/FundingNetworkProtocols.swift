import Foundation

typealias FundingNetworkViewProtocol = FundingScreenViewProtocol

@MainActor
protocol FundingNetworkPresenterProtocol: AnyObject {
    func setup()
    func chooseNetwork(_ id: String)
    func back()
}

@MainActor
protocol FundingNetworkInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol FundingNetworkWireframeProtocol: AnyObject {
    func show(_ screen: FundingFlowScreen)
    func back()
}

extension FundingFlowWireframe: FundingNetworkWireframeProtocol {}
