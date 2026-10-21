import Foundation

public enum DevicePermissionRequestError: Error {
    case osDenied
}

/// Handles `.deviceCapability` permissions: two-step flow that first asks the
/// user (via the prompt) and then requests the matching OS-level permission.
///
/// The OS dialog shows only when the app is in the foreground. A permission that the OS gave already needs no
/// screen. When no screen can show the OS dialog, the request fails with ``ProductPermissionPromptError``: that is
/// not a refusal.
public final class DeviceCapabilityPermissionHandler: Sendable {
    private let repository: ProductPermissionRepositoryProtocol
    private let requester: ProductPermissionRequesting
    private let osAsker: OSPermissionAsking
    private let foreground: ProductPromptForegroundWaiting
    private let foregroundWait: Duration

    public init(
        repository: ProductPermissionRepositoryProtocol,
        requester: ProductPermissionRequesting,
        osAsker: OSPermissionAsking,
        foreground: ProductPromptForegroundWaiting = AlwaysInForeground(),
        foregroundWait: Duration = ProductPermissionRequester.foregroundWait
    ) {
        self.repository = repository
        self.requester = requester
        self.osAsker = osAsker
        self.foreground = foreground
        self.foregroundWait = foregroundWait
    }

    public func isGranted(productId: String, capability: DeviceCapabilityType) async throws -> Bool {
        let osPermission = await osAsker.checkPermission(for: capability)

        guard osPermission.isAllowed else {
            // not granted yet on OS level
            return false
        }

        let permissionState = try await repository.getPermissionState(
            productId: productId,
            permission: .deviceCapability(capability)
        )

        return permissionState.isAllowed
    }

    public func request(productId: String, capability: DeviceCapabilityType) async throws -> Bool {
        let osPermission = await osAsker.checkPermission(for: capability)

        if osPermission.isDenied {
            // baned on OS level no way to proceed
            return false
        }

        let permission = ProductPermission.deviceCapability(capability)

        let permissionState = try await repository.getPermissionState(
            productId: productId,
            permission: permission
        )

        switch permissionState {
        case .allowedOnce,
             .allowedAlways:
            return try await promptOsPermissionIfNeeded(currentStatus: osPermission, capability: capability)
        case .denied:
            return false
        case .notDetermined:
            let isAllowed = try await promptAppLevelPermission(productId: productId, permission: permission)
            guard isAllowed else { return false }

            return try await promptOsPermissionIfNeeded(currentStatus: osPermission, capability: capability)
        }
    }

    public func requestDecision(
        productId: String,
        capability: DeviceCapabilityType
    ) async throws -> PermissionDecision {
        let osPermission = await osAsker.checkPermission(for: capability)
        guard !osPermission.isDenied else { throw DevicePermissionRequestError.osDenied }
        let permission = ProductPermission.deviceCapability(capability)
        let state = try await repository.getPermissionState(productId: productId, permission: permission)
        let decision: PermissionDecision =
            switch state {
            case .allowedAlways:
                .allowAlways
            case .denied:
                .deny
            case .allowedOnce,
                 .notDetermined:
                try await requester.prompt(productId: productId, permission: permission)
            }
        guard decision != .deny else { return .deny }
        guard try await promptOsPermissionIfNeeded(currentStatus: osPermission, capability: capability) else {
            throw DevicePermissionRequestError.osDenied
        }
        return decision
    }
}

private extension DeviceCapabilityPermissionHandler {
    func promptAppLevelPermission(productId: String, permission: ProductPermission) async throws -> Bool {
        let decision = try await requester.prompt(productId: productId, permission: permission)

        switch decision {
        case .allowAlways:
            try await repository.grant(productId: productId, permission: permission)
            return true
        case .allowOnce:
            repository.grantOneTime(productId: productId, permission: permission)
            return true
        case .deny:
            try await repository.deny(productId: productId, permission: permission)
            return false
        }
    }

    func promptOsPermissionIfNeeded(
        currentStatus: OSPermissionStatus,
        capability: DeviceCapabilityType
    ) async throws -> Bool {
        guard currentStatus.isNotDetermined else { return true }
        guard await foreground.waitForForeground(timeout: foregroundWait) else {
            throw ProductPermissionPromptError.notInForeground
        }
        return await osAsker.requestPermission(for: capability)
    }
}
