import Foundation

/// Authorizes the device owner from places without a wireframe, such as prompt view factories.
@MainActor
final class DeviceOwnerAuthorizer: AuthorizationPresentable {}
