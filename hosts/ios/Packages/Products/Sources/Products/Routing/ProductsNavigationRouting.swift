import Foundation

/// Product navigation: opening further product pages and external URLs. Refines
/// ``ProductsRouting`` so the facade anchors it the same way, though navigation
/// drives its own presentation (tab bar / web) rather than `present(view:)`.
@MainActor
public protocol ProductsNavigationRouting: ProductsRouting {
    func navigateTo(destination: ProductPage) async throws
    func openExternalURL(_ url: URL) async throws
    /// Hands a link to the app's own link handling, for a target the host answers itself
    /// (a Pocket link) rather than a page of the product.
    func openHostLink(_ url: URL) async throws
}
