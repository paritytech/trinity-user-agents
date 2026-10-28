import struct TrUAPIHost.ProductId
import enum TrUAPIHost.ExecutableKind
import Foundation

/// Manifest products and legacy ones share this shape, so callers branch on `executables`
/// rather than on whether a manifest was published.
public struct ResolvedProduct: Hashable, Sendable {
    public let id: ProductId
    public let displayName: String
    public let description: String?
    public let icon: ProductIcon?
    public let executables: ProductExecutables
    /// False for products that publish no root manifest, whose surfaces are discovered by probing
    /// the archive rather than by declaration.
    public let hasManifest: Bool

    public init(
        id: ProductId,
        displayName: String,
        description: String?,
        icon: ProductIcon?,
        executables: ProductExecutables,
        hasManifest: Bool
    ) {
        self.id = id
        self.displayName = displayName
        self.description = description
        self.icon = icon
        self.executables = executables
        self.hasManifest = hasManifest
    }

    /// A product with no manifest: one app rooted at the base name, named after its domain.
    ///
    /// Never the shape for a product whose manifest is published but broken — that one declared
    /// something, and serving the base name's archive instead would not be what it declared.
    public static func legacy(id: ProductId, displayName: String? = nil) -> ResolvedProduct {
        ResolvedProduct(
            id: id,
            displayName: displayName ?? id.rawValue,
            description: nil,
            icon: nil,
            executables: ProductExecutables(
                app: ProductExecutable.App(identifier: id, appVersion: .zero),
                widget: nil,
                worker: nil
            ),
            hasManifest: false
        )
    }

    public var product: Product {
        Product(id: id, name: displayName)
    }

    /// The archive `kind` is served from.
    public func contentId(for kind: ExecutableKind) throws -> ProductId {
        let identifier = switch kind {
        case .app: executables.app?.identifier
        case .widget: executables.widget?.identifier
        case .worker: executables.worker?.identifier
        }

        guard let identifier else { throw ProductResolutionError.missingExecutable(id, kind) }
        return identifier
    }
}
