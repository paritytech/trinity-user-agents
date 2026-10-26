import CoreLocation
import Foundation
import Products

protocol LocationAuthorizing: Sendable {
    func status() async -> OSPermissionStatus
    func requestWhenInUse() async -> Bool
}

final class LocationAuthorizationRequester: NSObject, LocationAuthorizing, @unchecked Sendable {
    private var manager: CLLocationManager?
    private var continuation: CheckedContinuation<Bool, Never>?

    func status() async -> OSPermissionStatus {
        await MainActor.run { OSPermissionStatus(locationStatus: CLLocationManager().authorizationStatus) }
    }

    func requestWhenInUse() async -> Bool {
        await withCheckedContinuation { continuation in
            Task { @MainActor in
                let manager = CLLocationManager()
                let current = OSPermissionStatus(locationStatus: manager.authorizationStatus)
                guard current.isNotDetermined else {
                    continuation.resume(returning: current.isAllowed)
                    return
                }
                self.manager = manager
                self.continuation = continuation
                manager.delegate = self
                manager.requestWhenInUseAuthorization()
            }
        }
    }
}

extension LocationAuthorizationRequester: CLLocationManagerDelegate {
    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        let status = OSPermissionStatus(locationStatus: manager.authorizationStatus)
        guard !status.isNotDetermined, let continuation else { return }

        self.continuation = nil
        self.manager = nil
        continuation.resume(returning: status.isAllowed)
    }
}

extension OSPermissionStatus {
    init(locationStatus: CLAuthorizationStatus) {
        switch locationStatus {
        case .authorizedAlways,
             .authorizedWhenInUse:
            self = .allowed
        case .denied,
             .restricted:
            self = .denied
        case .notDetermined:
            self = .notDetermined
        @unknown default:
            self = .notDetermined
        }
    }
}
