import struct TrUAPIHost.ProductId
import Foundation
import Operation_iOS
import PolkadotUI
import Products

actor ProductNameProvider {
    private nonisolated let host: ProductHost
    private nonisolated let productRepository: AnyDataProviderRepository<Product>
    private nonisolated let dotNsResolver: DotNsResolverProtocol?
    private nonisolated let productResolver: ProductResolving?
    private nonisolated let nameCache: ProductNameCaching
    private nonisolated let htmlParser: ProductLinkHTMLParsing
    private var currentTask: Task<Void, Never>?

    init(
        host: ProductHost,
        productRepository: AnyDataProviderRepository<Product>,
        dotNsResolver: DotNsResolverProtocol?,
        productResolver: ProductResolving?,
        nameCache: ProductNameCaching,
        htmlParser: ProductLinkHTMLParsing = ProductLinkHTMLParser()
    ) {
        self.host = host
        self.productRepository = productRepository
        self.dotNsResolver = dotNsResolver
        self.productResolver = productResolver
        self.nameCache = nameCache
        self.htmlParser = htmlParser
    }
}

extension ProductNameProvider: ChatProductNameProviding {
    nonisolated var identifier: String { host.toDotDomain().rawValue }

    nonisolated func provideName(
        _ completion: @escaping (String?) -> Void
    ) {
        Task { [weak self] in
            await self?.handleProvide(completion)
        }
    }

    nonisolated func cancel() {
        Task { [weak self] in
            await self?.handleCancel()
        }
    }
}

private extension ProductNameProvider {
    func handleProvide(
        _ completion: @escaping (String?) -> Void
    ) {
        currentTask?.cancel()

        currentTask = Task { [weak self] in
            guard let self else { return }

            let resolvedName = await resolveName()

            await MainActor.run {
                completion(resolvedName)
            }
        }
    }

    func handleCancel() {
        currentTask?.cancel()
        currentTask = nil
    }

    nonisolated func resolveName() async -> String {
        let domain = host.toDotDomain()
        let fallback = host.name

        if let cachedName = nameCache.name(for: domain) {
            return cachedName
        }

        switch await fetchManifestName(domain: domain) {
        case let .declared(name):
            nameCache.store(name: name, for: domain)
            return name
        case .unavailable:
            return fallback
        case .undeclared:
            break
        }

        let storedProduct = try? await productRepository
            .fetchOperation(by: { domain.rawValue }, options: RepositoryFetchOptions())
            .asyncExecute()

        let resolved: String =
            if let storedName = storedProduct?.name, storedName != fallback {
                storedName
            } else if let title = await fetchHTMLTitle(domain: domain) {
                title
            } else {
                storedProduct?.name ?? fallback
            }

        if resolved != fallback, !Task.isCancelled {
            nameCache.store(name: resolved, for: domain)
        }

        return resolved
    }

    enum ManifestName {
        case declared(String)
        case unavailable
        case undeclared
    }

    nonisolated func fetchManifestName(domain: ProductId) async -> ManifestName {
        guard let productResolver else { return .undeclared }

        do {
            let resolved = try await productResolver.resolve(domain)

            return resolved.hasManifest ? .declared(resolved.displayName) : .undeclared
        } catch {
            return .unavailable
        }
    }

    nonisolated func fetchHTMLTitle(domain: ProductId) async -> String? {
        guard
            let dotNsResolver,
            let contentDirectory = try? await dotNsResolver.resolveToLocalURL(dotNsName: domain.rawValue)
        else {
            return nil
        }
        let indexURL = contentDirectory.appendingPathComponent(ProductBundle.indexHTML)
        guard let html = try? String(contentsOf: indexURL, encoding: .utf8) else {
            return nil
        }
        return htmlParser.title(in: html)
    }
}
