import Foundation

/// Persisted record of a product's permission grant.
///
/// The `identifier` is a composite `"productId:typeName:key"` string and acts
/// as the primary key in CoreData. It is also used to key in-memory one-time
/// grants so both stores share the same key format.
public struct ProductPermissionGrant: Equatable, Sendable {
    public let productId: ProductId
    public let permission: ProductPermission
    public let granted: Bool
    public let grantedAt: Date?

    public var identifier: String {
        Self.makeIdentifier(productId: productId, permission: permission)
    }

    public init(
        productId: String,
        permission: ProductPermission,
        granted: Bool,
        grantedAt: Date?
    ) {
        self.productId = productId
        self.permission = permission
        self.granted = granted
        self.grantedAt = grantedAt
    }

    public static func makeIdentifier(
        productId: String,
        permission: ProductPermission
    ) -> String {
        "\(productId):\(permission.typeName):\(permission.key)"
    }
}
