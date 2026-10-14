import Foundation
import PolkadotUI

final class LinkedDevicesPresenter {
    weak var view: LinkedDevicesViewProtocol?

    private let interactor: LinkedDevicesInteractorInputProtocol
    private let wireframe: LinkedDevicesWireframeProtocol

    private var devices = [Chat.LocalDevice]()

    init(
        interactor: LinkedDevicesInteractorInputProtocol,
        wireframe: LinkedDevicesWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension LinkedDevicesPresenter: LinkedDevicesPresenterProtocol {
    func setup() {
        interactor.setup()
        provideViewModel()
    }

    func selectDevice(at index: Int) {
        guard index < devices.count else { return }
        wireframe.showDeviceDetails(from: view, device: devices[index])
    }

    func scanQRCode() {
        wireframe.showURLScan(
            from: view,
            delegate: self,
            initialMessage: String(localized: .linkedDevicesScanQRInstruction)
        )
    }

    func howItWorks() {
        // TODO: Navigate to how it works
    }
}

extension LinkedDevicesPresenter: URLScanDelegate {
    func urlScanDidReceiveResult(_ url: URL) {
        wireframe.completeOpeningURL(from: view, url: url)
    }
}

extension LinkedDevicesPresenter: LinkedDevicesInteractorOutputProtocol {
    func didReceiveDevices(_ devices: [Chat.LocalDevice]) {
        self.devices = devices
        provideViewModel()
    }
}

private extension LinkedDevicesPresenter {
    func provideViewModel() {
        if devices.isEmpty {
            provideEmptyViewModel()
        } else {
            provideDevicesViewModel()
        }
    }

    func provideEmptyViewModel() {
        let emptyModel = LinkedDevicesViewLayout.EmptyViewModel(
            title: String(localized: .linkedDevicesEmptyTitle),
            subtitle: String(localized: .linkedDevicesEmptySubtitle),
            scanButtonTitle: String(localized: .linkedDevicesEmptyScanButton),
            footerText: String(localized: .linkedDevicesEmptyFooter),
            howItWorksTitle: String(localized: .linkedDevicesEmptyHowItWorks)
        )
        view?.didReceive(viewModel: .empty(emptyModel))
    }

    func provideDevicesViewModel() {
        let items = devices.map { device in
            LinkedDevicesViewLayout.DeviceItem(
                id: device.identifier,
                icon: nil,
                name: device.displayDeviceName,
                subtitle: device.displayHostName
            )
        }

        let sectionHeader = LinkedDevicesViewLayout.DeviceSectionHeader(
            title: String(localized: .linkedDevicesSectionTitle),
            count: String(localized: .linkedDevicesSectionCount(devices.count))
        )

        let devicesModel = LinkedDevicesViewLayout.DevicesViewModel(
            sectionHeader: sectionHeader,
            items: items
        )
        view?.didReceive(viewModel: .devices(devicesModel))
    }
}
