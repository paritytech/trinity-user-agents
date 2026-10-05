import Foundation
import Products
import TrUAPIHost

extension OSPermissionAsking {
    func corePermissionStatus(_ request: HostDevicePermissionRequest) async -> DevicePermissionStatus {
        switch request {
        case .camera, .microphone, .notifications:
            switch await checkPermission(for: request.deviceCapabilityType) {
            case .allowed: .granted
            case .denied: .denied
            case .notDetermined: .notDetermined
            }
        case .location: .notDetermined
        case .bluetooth, .nfc, .clipboard, .openUrl, .biometrics: .notApplicable
        }
    }

    /// Product consent was consumed by the container before WebKit asks its delegate.
    func makeDeviceCapabilityHandler() -> JSDeviceCapabilityHandler {
        { capability in
            switch await self.checkPermission(for: capability.deviceCapabilityType) {
            case .allowed:
                .allowed
            case .denied:
                .denied
            case .notDetermined:
                await self.requestPermission(for: capability.deviceCapabilityType) ? .allowed : .denied
            }
        }
    }
}
