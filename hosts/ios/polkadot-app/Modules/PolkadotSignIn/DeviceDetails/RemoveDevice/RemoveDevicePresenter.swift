import Foundation

@MainActor
final class RemoveDevicePresenter {
    weak var view: RemoveDeviceViewProtocol?

    private let interactor: RemoveDeviceInteractorInputProtocol
    private let wireframe: RemoveDeviceWireframeProtocol
    private let device: Chat.LocalDevice
    private let onResult: (Bool) -> Void

    init(
        interactor: RemoveDeviceInteractorInputProtocol,
        wireframe: RemoveDeviceWireframeProtocol,
        device: Chat.LocalDevice,
        onResult: @escaping (Bool) -> Void
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.device = device
        self.onResult = onResult
    }
}

extension RemoveDevicePresenter: RemoveDevicePresenterProtocol {
    func setup() {
        let description = "\(device.displayHostName) (\(device.displayDeviceName))"
        view?.didReceive(deviceDescription: description)
    }

    func confirm() {
        view?.didReceive(isLoading: true)
        interactor.removeDevice(identifier: device.identifier)
    }

    func cancel() {
        wireframe.close(view: view, completion: nil)
    }
}

extension RemoveDevicePresenter: RemoveDeviceInteractorOutputProtocol {
    func didRemoveDevice() {
        wireframe.close(view: view) { [onResult] in
            onResult(true)
        }
    }

    func didFailToRemoveDevice(error _: Error) {
        wireframe.close(view: view) { [onResult] in
            onResult(false)
        }
    }
}
