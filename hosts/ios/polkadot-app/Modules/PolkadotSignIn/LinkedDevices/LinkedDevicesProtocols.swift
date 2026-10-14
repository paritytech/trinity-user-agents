import Foundation
import PolkadotUI
import UIKitExt

protocol LinkedDevicesViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: LinkedDevicesViewLayout.ViewModel)
}

@MainActor
protocol LinkedDevicesPresenterProtocol: AnyObject {
    func setup()
    func selectDevice(at index: Int)
    func scanQRCode()
    func howItWorks()
}

protocol LinkedDevicesInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol LinkedDevicesInteractorOutputProtocol: AnyObject {
    func didReceiveDevices(_ devices: [Chat.LocalDevice])
}

@MainActor
protocol LinkedDevicesWireframeProtocol: AlertPresentable, ErrorPresentable, ScanURLPresentable {
    func showDeviceDetails(from view: LinkedDevicesViewProtocol?, device: Chat.LocalDevice)
    func completeOpeningURL(from view: LinkedDevicesViewProtocol?, url: URL)
}
