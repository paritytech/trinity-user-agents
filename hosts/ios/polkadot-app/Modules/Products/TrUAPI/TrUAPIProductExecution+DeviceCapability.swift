import Foundation
import Products
import TrUAPIHost

extension OSPermissionAsking {
    /// Capture is owned by the core's native Media runtime, never by guest WebKit.
    /// Motion retains its user-gesture-bound product permission check.
    func makeDeviceCapabilityHandler(
        execution: TrUAPIProductExecutionProtocol
    ) -> JSDeviceCapabilityHandler {
        { capability in
            if case .motion = capability {
                return try await execution.authorizeDevicePermission(.motion) ? .allowed : .denied
            }
            return .denied
        }
    }
}
