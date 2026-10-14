import AVFoundation
import UIKit

enum CallMicrophoneAccess: Equatable {
    case granted
    case refused
    case denied
    case deferred
}

enum MicrophonePromptPolicy {
    case whenActive
    case always
}

protocol CallPermissionsServicing: AnyObject {
    var isMicrophoneGranted: Bool { get }
    var isCameraGranted: Bool { get }
    var isCameraDenied: Bool { get }

    var isMicrophoneDenied: Bool { get }

    func resolveMicrophoneAccess(prompting policy: MicrophonePromptPolicy) async -> CallMicrophoneAccess

    func ensurePermissions() async -> Bool
    func ensureCameraAccess() async -> Bool
}

final class CallPermissionsService {
    private let applicationStateProvider: @MainActor () -> UIApplication.State
    private let recordPermissionProvider: RecordPermissionProviding
    private let recordPermissionRequester: RecordPermissionRequesting
    private let cameraPermissionService: CameraPermissionServicing

    init(
        applicationStateProvider: @escaping @MainActor () -> UIApplication.State = {
            UIApplication.shared.applicationState
        },
        recordPermissionProvider: RecordPermissionProviding = RecordPermissionService(),
        recordPermissionRequester: RecordPermissionRequesting = RecordPermissionService(),
        cameraPermissionService: CameraPermissionServicing = CameraPermissionService()
    ) {
        self.applicationStateProvider = applicationStateProvider
        self.recordPermissionProvider = recordPermissionProvider
        self.recordPermissionRequester = recordPermissionRequester
        self.cameraPermissionService = cameraPermissionService
    }
}

private extension CallPermissionsService {
    // An inactive/backgrounded app (e.g. a locked-screen CallKit answer) can't
    // present the system permission prompt. Awaiting one there stalls the call
    // instead of surfacing a dialog, so only prompt when the app is active.
    @MainActor
    var canPresentPermissionPrompt: Bool {
        applicationStateProvider() == .active
    }

    func canPrompt(with policy: MicrophonePromptPolicy) async -> Bool {
        switch policy {
        case .always:
            true
        case .whenActive:
            await canPresentPermissionPrompt
        }
    }
}

extension CallPermissionsService: CallPermissionsServicing {
    var isMicrophoneGranted: Bool {
        recordPermissionProvider.recordPermission == .granted
    }

    var isMicrophoneDenied: Bool {
        recordPermissionProvider.recordPermission == .denied
    }

    func resolveMicrophoneAccess(prompting policy: MicrophonePromptPolicy) async -> CallMicrophoneAccess {
        switch recordPermissionProvider.recordPermission {
        case .granted:
            return .granted
        case .denied:
            return .denied
        case .undetermined:
            guard await canPrompt(with: policy) else {
                return .deferred
            }

            return await recordPermissionRequester.requestRecordPermission() ? .granted : .refused
        @unknown default:
            return .denied
        }
    }

    var isCameraGranted: Bool {
        cameraPermissionService.permission() == .authorized
    }

    var isCameraDenied: Bool {
        cameraPermissionService.permission() == .denied
    }

    func ensurePermissions() async -> Bool {
        await resolveMicrophoneAccess(prompting: .whenActive) == .granted
    }

    func ensureCameraAccess() async -> Bool {
        switch cameraPermissionService.permission() {
        case .authorized:
            return true
        case .notDetermined:
            guard await canPresentPermissionPrompt else {
                return false
            }

            return await cameraPermissionService.requestPermission() == .authorized
        case .denied:
            return false
        }
    }
}
