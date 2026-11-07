import Foundation
import IssueMonitoring
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
        hostProvider: ProductHostProviding,
        telemetry: HostLoadTelemetry = .disabled
    ) -> ProductResolver {
        ProductResolver(
            metadataRepository: metadataRepository,
            hostProvider: hostProvider,
            logger: logger,
            screening: screening,
            telemetry: telemetry
        )
    }
}
