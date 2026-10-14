import Foundation
import Kingfisher
@preconcurrency import Products
import PolkadotUI
import SwiftUI

enum ProductIconImageDataProviderError: Error {
    case iconUnavailable
}

/// One product icon, wherever it comes from: the CID its root manifest pins, the `<link rel=icon>`
/// a legacy product ships in its archive, or a rendered letter avatar when neither resolves.
final class ProductIconImageDataProvider {
    let cacheKey: String

    private let domain: String
    private let dotNsResolver: DotNsResolverProtocol
    private let iconLoader: ProductIconLoading
    private let htmlParser: ProductLinkHTMLParsing
    private let logger: LoggerProtocol

    init(
        domain: String,
        dotNsResolver: DotNsResolverProtocol,
        iconLoader: ProductIconLoading,
        htmlParser: ProductLinkHTMLParsing,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.domain = domain
        self.dotNsResolver = dotNsResolver
        self.iconLoader = iconLoader
        self.htmlParser = htmlParser
        self.logger = logger
        cacheKey = "ProductIcon-\(domain)"
    }
}

extension ProductIconImageDataProvider: ImageDataProvider, @unchecked Sendable {
    func data(handler: @escaping (Result<Data, Error>) -> Void) {
        Task(priority: .medium) {
            do {
                try await handler(.success(loadIconData()))
            } catch {
                logger.error("Failed to load product icon for \(domain): \(error)")

                // A product with no icon and one whose gateway is unreachable look the same to the
                // user, and both are better served by a letter than by an empty circle.
                guard let fallback = await makeLetterIconData() else {
                    handler(.failure(error))
                    return
                }

                handler(.success(fallback))
            }
        }
    }
}

private extension ProductIconImageDataProvider {
    func loadIconData() async throws -> Data {
        if let declared = await iconLoader.loadIcon(for: domain) {
            return declared
        }

        // No manifest icon: legacy products link theirs from the archive's index.html.
        let contentDirectory = try await dotNsResolver.resolveToLocalURL(dotNsName: domain)
        let html = try String(
            contentsOf: contentDirectory.appendingPathComponent(ProductBundle.indexHTML),
            encoding: .utf8
        )

        guard let raw = htmlParser.iconRelativePath(in: html) else {
            throw ProductIconImageDataProviderError.iconUnavailable
        }

        return try Data(contentsOf: contentDirectory.appendingPathComponent(raw))
    }

    @MainActor
    func makeLetterIconData() -> Data? {
        let viewModel = AvatarViewModel.colored(
            text: String(domain.prefix(1)),
            colorSeed: domain
        )

        let renderer = ImageRenderer(content: DSAvatar(viewModel: viewModel, size: .s64))
        renderer.scale = UIScreen.main.scale

        return renderer.uiImage?.pngData()
    }
}
