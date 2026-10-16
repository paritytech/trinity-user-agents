import Foundation

public protocol ProductHostProviding: Sendable {
    /// Sync construction from an already-suffixed host string.
    /// Returns nil while the TLD is unknown, and kicks a background refresh.
    func host(rawString: String) -> ProductHost?
    func host(url: URL) -> ProductHost?
    func host(navigationDestination: String) -> ProductHost?
    func page(url: URL) -> ProductPage?
    func page(navigationDestination: String) -> ProductPage?

    /// Sync construction from a bare label ("browse"); appends the chain TLD first.
    func host(label: String) -> ProductHost?

    /// Awaits the TLD, then constructs from a bare label. For gated entry points.
    func resolveHost(label: String) async throws -> ProductHost?

    /// Awaits the TLD, then constructs from an already-suffixed host string.
    func resolveHost(rawString: String) async throws -> ProductHost?

    /// Awaits the TLD, then parses a product destination into a page.
    /// Throws ``ProductPageResolutionError`` when the TLD is unknown or the destination is not on this network.
    func resolvePage(destination: String) async throws -> ProductPage
}

public final class ProductHostFactory: ProductHostProviding {
    private let tldProvider: DotNsTldProviding

    public init(tldProvider: DotNsTldProviding) {
        self.tldProvider = tldProvider
    }

    public func host(rawString: String) -> ProductHost? {
        tldProvider.currentTld().flatMap { ProductHost.parse(rawString, tld: $0) }
    }

    public func host(url: URL) -> ProductHost? {
        tldProvider.currentTld().flatMap { ProductHost.fromUrl(url, tld: $0) }
    }

    public func host(navigationDestination: String) -> ProductHost? {
        tldProvider.currentTld().flatMap { ProductHost.fromNavigationDestination(navigationDestination, tld: $0) }
    }

    public func page(url: URL) -> ProductPage? {
        tldProvider.currentTld().flatMap { ProductPage.fromUrl(url, tld: $0) }
    }

    public func page(navigationDestination: String) -> ProductPage? {
        tldProvider.currentTld().flatMap { ProductPage.fromNavigationDestination(navigationDestination, tld: $0) }
    }

    public func host(label: String) -> ProductHost? {
        tldProvider.currentTld().flatMap { ProductHost(name: label, root: $0) }
    }

    public func resolveHost(label: String) async throws -> ProductHost? {
        let tld = try await tldProvider.resolveTld()
        return ProductHost(name: label, root: tld)
    }

    public func resolveHost(rawString: String) async throws -> ProductHost? {
        let tld = try await tldProvider.resolveTld()
        return ProductHost.parse(rawString, tld: tld)
    }

    public func resolvePage(destination: String) async throws -> ProductPage {
        let tld: String

        do {
            tld = try await tldProvider.resolveTld()
        } catch let error as CancellationError {
            throw error
        } catch {
            throw ProductPageResolutionError.tldUnavailable(underlying: error)
        }

        guard let page = ProductPage.fromNavigationDestination(destination, tld: tld) else {
            throw ProductPageResolutionError.destinationNotOnNetwork(destination: destination, tld: tld)
        }

        return page
    }
}
