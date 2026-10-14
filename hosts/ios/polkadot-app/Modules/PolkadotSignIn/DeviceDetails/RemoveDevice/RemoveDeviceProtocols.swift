import PolkadotUI
import UIKitExt

protocol RemoveDeviceViewProtocol: ControllerBackedProtocol {
    func didReceive(deviceDescription: String)
    func didReceive(isLoading: Bool)
}

@MainActor
protocol RemoveDevicePresenterProtocol: AnyObject {
    func setup()
    func confirm()
    func cancel()
}

protocol RemoveDeviceInteractorInputProtocol: AnyObject {
    func removeDevice(identifier: String)
}

@MainActor
protocol RemoveDeviceInteractorOutputProtocol: AnyObject {
    func didRemoveDevice()
    func didFailToRemoveDevice(error: Error)
}

@MainActor
protocol RemoveDeviceWireframeProtocol: AnyObject {
    func close(view: RemoveDeviceViewProtocol?, completion: (() -> Void)?)
}
