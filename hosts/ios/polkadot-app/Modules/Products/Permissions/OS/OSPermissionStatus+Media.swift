import Foundation
import AVFoundation
import Products

extension OSPermissionStatus {
    init(mediaStatus: AVAuthorizationStatus) {
        switch mediaStatus {
        case .notDetermined:
            self = .notDetermined
        case .denied,
             .restricted:
            self = .denied
        case .authorized:
            self = .allowed
        @unknown default:
            self = .notDetermined
        }
    }
}
