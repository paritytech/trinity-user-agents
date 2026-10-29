import Products
import TrUAPIHost

/// The OS gate behind a device capability, read without prompting. The core
/// combines it with the product's stored decision, on a product's own request
/// and on the settings screens alike.
enum TrUAPIDevicePermissionStatus {
    static func resolve(
        _ request: HostDevicePermissionRequest,
        osPermissionAsker: OSPermissionAsking
    ) async -> DevicePermissionStatus {
        switch request {
        case .camera,
             .microphone,
             .notifications:
            switch await osPermissionAsker.checkPermission(for: request.deviceCapabilityType) {
            case .allowed: .granted
            case .denied: .denied
            case .notDetermined: .notDetermined
            }
        case .location:
            .notDetermined
        case .bluetooth,
             .nfc,
             .clipboard,
             .openUrl,
             .biometrics,
             .motion:
            .notApplicable
        }
    }
}
