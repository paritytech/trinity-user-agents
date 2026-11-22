import Foundation
import Products
import TrUAPIHost

extension OSPermissionAsking {
    /// Direct camera/microphone capture belongs to Media. Motion remains a
    /// WebKit user-gesture capability authorized through the product execution.
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
