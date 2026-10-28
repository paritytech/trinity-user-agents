import Foundation
import SDKLogger

public struct ProductResolverFactory {
    let screening: PocketCardScreening
    let logger: SDKLoggerProtocol

    public init(screening: PocketCardScreening, logger: SDKLoggerProtocol) {
        self.screening = screening
        self.logger = logger
    }

    func makeResolver(
        metadataRepository: ProductMetadataRepositoryProtocol,
        hostProvider: ProductHostProviding
    ) -> ProductResolver {
        ProductResolver(
            metadataRepository: metadataRepository,
            hostProvider: hostProvider,
            logger: logger,
            screening: screening
        )
    }
}
