import Foundation

final class LinkedDevicesInteractor {
    weak var presenter: LinkedDevicesInteractorOutputProtocol?

    private let deviceDataProviderFactory: LocalDeviceDataProviderMaking

    private var devicesSubscriptionTask: Task<Void, Never>?

    init(deviceDataProviderFactory: LocalDeviceDataProviderMaking = LocalDeviceDataProviderFactory()) {
        self.deviceDataProviderFactory = deviceDataProviderFactory
    }

    deinit {
        unsubscribeFromDevices()
    }
}

extension LinkedDevicesInteractor: LinkedDevicesInteractorInputProtocol {
    func setup() {
        subscribeToDevices()
    }
}

private extension LinkedDevicesInteractor {
    func subscribeToDevices() {
        devicesSubscriptionTask = Task { [weak self] in
            guard let sequence = self?.deviceDataProviderFactory.subscribeDevices() else {
                return
            }
            for await devices in sequence {
                await self?.reportNewDevices(devices)
            }
        }
    }

    func unsubscribeFromDevices() {
        devicesSubscriptionTask?.cancel()
        devicesSubscriptionTask = nil
    }

    @MainActor
    func reportNewDevices(_ devices: [Chat.LocalDevice]) {
        presenter?.didReceiveDevices(devices)
    }
}
