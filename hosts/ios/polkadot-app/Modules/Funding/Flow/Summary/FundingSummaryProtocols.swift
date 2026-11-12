import Foundation
import TrUAPIHost

typealias FundingSummaryViewProtocol = FundingScreenViewProtocol

@MainActor
protocol FundingSummaryPresenterProtocol: AnyObject {
    func setup()
    func start()
    func showFees()
    func showCountry()
    func showProviders()
    func back()
}

@MainActor
protocol FundingSummaryInteractorInputProtocol: AnyObject {
    func setup()
    func start(provider: ProductId) async throws
}

@MainActor
protocol FundingSummaryWireframeProtocol: AnyObject {
    func show(_ screen: FundingFlowScreen)
    func showProgress()
    func back()
}

extension FundingFlowWireframe: FundingSummaryWireframeProtocol {}
