import Foundation
import Products

// MARK: - Permissions & Push Notifications

extension ProductsNativeApi {
    func requestDevicePermission(capability capabilityString: String) async throws -> Bool {
        guard let capability = DeviceCapabilityType(rawValue: capabilityString) else {
            return false
        }

        let permission = ProductPermission.deviceCapability(capability)
        return try await permissionGuard.requestPermission(
            productId: productId,
            permission: permission
        )
    }

    func requestRemotePermissions(_ requests: [RemotePermissionRequest]) async throws -> Bool {
        let permissions = requests.flatMap { $0.toDomainPermissions() }
        return try await permissionGuard.requestPermissionsBatched(
            productId: productId,
            permissions: permissions
        )
    }

    func allowWebRtcAccess() async throws -> Bool {
        try await permissionGuard.consumePermission(
            productId: productId,
            permission: .webRtcAccess
        )
    }

    func pushNotification(_ request: ScheduledNotificationRequest) async throws -> UInt32 {
        guard try await permissionGuard.consumePermission(
            productId: productId,
            permission: .deviceCapability(.notifications)
        ) else {
            throw ProductNativeApiError.permissionDenied
        }

        return try await notificationScheduler.schedule(
            productId: productId,
            request: request
        )
    }

    func cancelPushNotification(identifier: UInt32) async throws {
        try await notificationScheduler.cancel(
            productId: productId,
            notificationId: identifier
        )
    }
}
