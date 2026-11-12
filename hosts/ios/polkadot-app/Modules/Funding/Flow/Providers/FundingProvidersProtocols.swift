import Foundation
import TrUAPIHost

typealias FundingProvidersViewProtocol = FundingScreenViewProtocol

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
