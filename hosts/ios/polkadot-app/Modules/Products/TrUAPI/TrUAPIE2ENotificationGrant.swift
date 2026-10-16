import Foundation
import TrUAPIHost

/// The switch of the truapi E2E harness that grants products the notification permission
/// (`TRUAPI_IOS_E2E_GRANT_NOTIFICATIONS=1`, S54). The harness cannot answer the system prompt, so the app has no OS
/// permission. Off outside a simulator build.
enum TrUAPIE2ENotificationGrant {
    static let isOn: Bool = {
        #if targetEnvironment(simulator)
            ProcessInfo.processInfo.environment["TRUAPI_IOS_E2E_GRANT_NOTIFICATIONS"] == "1"
        #else
            false
        #endif
    }()

    /// The harness also gave the app the OS permission (`TRUAPI_IOS_E2E_OS_NOTIFICATIONS=1`, A1), so a notification
    /// goes to the OS and shows. Off outside a simulator build.
    static let osPermissionGranted: Bool = {
        #if targetEnvironment(simulator)
            ProcessInfo.processInfo.environment["TRUAPI_IOS_E2E_OS_NOTIFICATIONS"] == "1"
        #else
            false
        #endif
    }()
}

extension HostDevicePermissionRequest {
    /// True when the harness grants this permission without a prompt.
    var isGrantedByE2EHarness: Bool {
        guard case .notifications = self else { return false }
        return TrUAPIE2ENotificationGrant.isOn
    }
}
