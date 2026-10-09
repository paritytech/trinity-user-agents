import Foundation
import Products
import TrUAPIHost

extension OSPermissionAsking {
    /// Product consent for camera and microphone was consumed by the container
    /// before WebKit asks its delegate. Motion has no container gate, because
    /// WebKit only consults its delegate during the product's user gesture, so
    /// the product's decision is resolved here through `execution`.
    func makeDeviceCapabilityHandler(
        execution: TrUAPIProductExecutionProtocol
    ) -> JSDeviceCapabilityHandler {
        { capability in
            if case .motion = capability {
                return try await execution.authorizeDevicePermission(.motion) ? .allowed : .denied
            }
            switch await self.checkPermission(for: capability.deviceCapabilityType) {
            case .allowed:
                return .allowed
            case .denied:
                return .denied
            case .notDetermined:
                return await self.requestPermission(for: capability.deviceCapabilityType) ? .allowed : .denied
            }
        }
    }
}
