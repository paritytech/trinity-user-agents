import Foundation
import PolkadotUI

protocol DeviceDetailsViewModelMaking {
    func makeViewModel(from device: Chat.LocalDevice) -> DeviceDetailsViewLayout.ViewModel
}

final class DeviceDetailsViewModelFactory {
    private lazy var dateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .none
        return formatter
    }()
}

extension DeviceDetailsViewModelFactory: DeviceDetailsViewModelMaking {
    func makeViewModel(from device: Chat.LocalDevice) -> DeviceDetailsViewLayout.ViewModel {
        DeviceDetailsViewLayout.ViewModel(
            deviceValue: device.displayDeviceName,
            hostValue: device.displayHostName,
            addedValue: makeAddedValue(from: device.createdAt)
        )
    }
}

private extension DeviceDetailsViewModelFactory {
    func makeAddedValue(from createdAt: Date) -> String {
        if Calendar.current.isDateInToday(createdAt) {
            return String(localized: .linkedDevicesDeviceDetailsToday)
        }

        if Calendar.current.isDateInYesterday(createdAt) {
            return String(localized: .linkedDevicesDeviceDetailsYesterday)
        }

        return dateFormatter.string(from: createdAt)
    }
}
