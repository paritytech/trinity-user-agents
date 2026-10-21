import Foundation
import KeyDerivation
import Products

/// Serves the chat product (`chat.<tld>`) from the files installed for it, whatever dotNS publishes under that name.
/// The label reaches the core's `PersonChat`, so a published release reaches those files only through
/// ``ChatProductReleases``, after its archive, version and entry file pass their checks (HX4).
struct BundledChatProductResolver: ProductResolving {
    let resolver: ProductResolving

    func resolve(_ productId: ProductId) async throws -> ResolvedProduct {
        guard Self.isChatProduct(productId) else { return try await resolver.resolve(productId) }

        // No published executables: the worker comes from the files `BundledChatProduct` installs.
        return ResolvedProduct(
            id: productId,
            displayName: "Chats",
            description: nil,
            icon: nil,
            executables: ProductExecutables(app: nil, widget: nil, worker: nil),
            hasManifest: false
        )
    }

    static func isChatProduct(_ productId: ProductId) -> Bool {
        guard let tld = productId.components(separatedBy: ".").last, !tld.isEmpty else { return false }
        return BuiltInProduct.chat(for: tld) == productId
    }
}
