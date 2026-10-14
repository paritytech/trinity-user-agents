import Foundation
import PolkadotUI

@MainActor
final class DeviceDetailsPresenter {
    weak var view: DeviceDetailsViewProtocol?

    private let wireframe: DeviceDetailsWireframeProtocol
    private let viewModelFactory: DeviceDetailsViewModelMaking
    private let device: Chat.LocalDevice

    init(
        wireframe: DeviceDetailsWireframeProtocol,
        viewModelFactory: DeviceDetailsViewModelMaking,
        device: Chat.LocalDevice
    ) {
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.device = device
    }
}

extension DeviceDetailsPresenter: DeviceDetailsPresenterProtocol {
    func setup() {
        let viewModel = viewModelFactory.makeViewModel(from: device)
        view?.didReceive(viewModel: viewModel)
    }

    func removeDevice() {
        wireframe.showRemoveDevice(
            from: view,
            device: device
        ) { [weak self] success in
            guard let self else { return }

            if success {
                wireframe.close(from: view)
            } else {
                view?.controller.showToast(
                    message: String(localized: .linkedDevicesSignInError),
                    type: .error
                )
            }
        }
    }
}
