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

    /// A product with no manifest: one app rooted at the base name, named after its domain. Also
    /// the degraded shape when the chain cannot be reached, which keeps cached content usable.
    ///
    /// Never the shape for a product whose manifest is published but broken — that one declared
    /// something, and serving the base name's archive instead would not be what it declared.
    public static func legacy(id: ProductId, displayName: String? = nil) -> ResolvedProduct {
        ResolvedProduct(
            id: id,
            displayName: displayName ?? id,
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

    /// Name the app's content archive resolves under; the base name for legacy products.
    public var appContentId: ProductId {
        executables.app?.identifier ?? id
    }

    /// The archive `kind` is served from.
    ///
    /// A product that publishes no executable of that kind is served from its
    /// base name, which is where a legacy product and a script installed by
    /// hand through debug settings both live. Everything that asks which
    /// archive backs a surface asks here, so no two callers can disagree.
    public func contentId(for kind: ExecutableKind) -> ProductId {
        switch kind {
        case .app: appContentId
        case .widget: executables.widget?.identifier ?? id
        case .worker: executables.worker?.identifier ?? id
        }
    }
}
