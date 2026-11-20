import Foundation

typealias FundingFeesViewProtocol = FundingScreenViewProtocol

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
