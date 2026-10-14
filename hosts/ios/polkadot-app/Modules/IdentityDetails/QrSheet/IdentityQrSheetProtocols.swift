import UIKitExt
import PolkadotUI

protocol IdentityQrSheetViewProtocol: ControllerBackedProtocol {
    var viewModel: IdentityDetailsViewModel { get }
}

@MainActor
protocol IdentityQrSheetPresenterProtocol: AnyObject {
    func setup()
    func close()
}

@MainActor
protocol IdentityQrSheetWireframeProtocol: AnyObject {
    func close(from view: IdentityQrSheetViewProtocol?)
}
