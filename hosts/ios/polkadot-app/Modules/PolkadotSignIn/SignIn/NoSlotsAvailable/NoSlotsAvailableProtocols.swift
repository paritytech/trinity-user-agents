import Foundation
import UIKitExt

protocol NoSlotsAvailableViewProtocol: ControllerBackedProtocol {
    func didReceive(message: String)
}

@MainActor
protocol NoSlotsAvailablePresenterProtocol: AnyObject {
    func setup()
    func dismiss()
}

@MainActor
protocol NoSlotsAvailableWireframeProtocol: AnyObject {
    func close(view: NoSlotsAvailableViewProtocol?)
}
