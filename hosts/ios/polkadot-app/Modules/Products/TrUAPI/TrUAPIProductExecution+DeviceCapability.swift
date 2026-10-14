import Foundation
import Products

extension OSPermissionAsking {
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
