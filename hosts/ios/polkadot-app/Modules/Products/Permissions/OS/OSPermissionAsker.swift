import AlarmKit
import AVFoundation
import Foundation
import Products

final class OSPermissionAsker: @unchecked Sendable {
    private let notificationService: UserNotificationServicing

    init(notificationService: UserNotificationServicing = UserNotificationService.shared) {
        self.notificationService = notificationService
    }
}

extension OSPermissionAsker: OSPermissionAsking {
    func checkPermission(for capability: DeviceCapabilityType) async -> OSPermissionStatus {
        switch capability {
        case .notifications:
            await checkNotificationStatus()
        case .camera:
            checkCaptureDeviceStatus(.video)
        case .microphone:
            checkCaptureDeviceStatus(.audio)
        case .bluetooth,
             .nfc,
             .location,
             .clipboard,
             .openUrl,
             .biometrics:
            // TODO: These capabilities require a native bridge to be usable from
            // the WKWebView sandbox. Return true for now; actual OS permission
            // prompts will be added when the bridge is implemented.
            .notDetermined
        }
    }

    func requestPermission(for capability: DeviceCapabilityType) async -> Bool {
        switch capability {
        case .notifications:
            await askNotifications()
        case .camera:
            await askCaptureDevice(.video)
        case .microphone:
            await askCaptureDevice(.audio)
        case .bluetooth,
             .nfc,
             .location,
             .clipboard,
             .openUrl,
             .biometrics:
            // TODO: These capabilities require a native bridge to be usable from
            // the WKWebView sandbox. Return true for now; actual OS permission
            // prompts will be added when the bridge is implemented.
            true
        }
    }
}

extension OSPermissionAsker: ReminderPermissionAsking {
    func askNotifications() async -> Bool {
        await withCheckedContinuation { continuation in
            notificationService.requestNotificationsAuthorization { granted in
                continuation.resume(returning: granted)
            }
        }
    }

    func askAlarm() async -> Bool {
        guard #available(iOS 26.1, *) else {
            return false
        }
        switch alarmAuthorizationStatus() {
        case .allowed:
            return true
        case .notDetermined:
            return await (try? AlarmManager.shared.requestAuthorization()) == .authorized
        case .denied:
            return false
        }
    }
}

private extension OSPermissionAsker {
    func checkNotificationStatus() async -> OSPermissionStatus {
        let notificationStatus = await notificationService.notificationAccessStatus()
        return OSPermissionStatus(notificationStatus: notificationStatus)
    }

    @available(iOS 26.1, *)
    func alarmAuthorizationStatus() -> OSPermissionStatus {
        switch AlarmManager.shared.authorizationState {
        case .authorized:
            .allowed
        case .denied:
            .denied
        case .notDetermined:
            .notDetermined
        @unknown default:
            .notDetermined
        }
    }

    func askCaptureDevice(_ mediaType: AVMediaType) async -> Bool {
        await AVCaptureDevice.requestAccess(for: mediaType)
    }

    func checkCaptureDeviceStatus(_ mediaType: AVMediaType) -> OSPermissionStatus {
        let status = AVCaptureDevice.authorizationStatus(for: mediaType)

        return OSPermissionStatus(mediaStatus: status)
    }
}
