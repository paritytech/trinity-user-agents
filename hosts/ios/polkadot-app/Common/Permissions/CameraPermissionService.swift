import AVFoundation

enum CameraPermissionResult {
    case authorized
    case denied
    case notDetermined
}

protocol CameraPermissionServicing: AnyObject {
    func permission() -> CameraPermissionResult
    func requestPermission() async -> CameraPermissionResult
}

final class CameraPermissionService: CameraPermissionServicing {
    func permission() -> CameraPermissionResult {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            .authorized
        case .notDetermined:
            .notDetermined
        case .denied,
             .restricted:
            .denied
        @unknown default:
            .denied
        }
    }

    func requestPermission() async -> CameraPermissionResult {
        let value = permission()
        guard value == .notDetermined else { return value }
        return await AVCaptureDevice.requestAccess(for: .video) ? .authorized : .denied
    }
}
