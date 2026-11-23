import Foundation
import UIKitExt
import TrUAPIHost

@MainActor
protocol FundingCountryViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: FundingCountryViewModel)
}

@MainActor
protocol FundingCountryPresenterProtocol: AnyObject {
    func setup()
    func chooseCountry(_ country: PaymentCountry)
    func back()
}

@MainActor
protocol FundingCountryInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol FundingCountryWireframeProtocol: AnyObject {
    func back()
}

extension FundingFlowWireframe: FundingCountryWireframeProtocol {}
