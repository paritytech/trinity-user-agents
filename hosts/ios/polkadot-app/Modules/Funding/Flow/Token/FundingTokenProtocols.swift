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
    /// Has the core hand the session to the latest ask's best quote once
    /// every provider answered.
    func startWhenQuoted() async throws
}

@MainActor
protocol FundingTokenWireframeProtocol: AnyObject {
    func show(_ screen: FundingFlowScreen)
    func back()
}

extension FundingFlowWireframe: FundingTokenWireframeProtocol {}
