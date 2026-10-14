import Foundation
import Operation_iOS
import SDKLogger
import EventCenter

public protocol RuntimeProviderFactoryProtocol {
    func createRuntimeProvider(for chain: ChainModel) -> RuntimeProviderProtocol
}

public final class RuntimeProviderFactory {
    public let fileOperationFactory: RuntimeFilesOperationFactoryProtocol
    public let repository: AnyDataProviderRepository<RuntimeMetadataItem>
    public let dataOperationFactory: DataOperationFactoryProtocol
    public let signedExtensionCodersFactory: SignedExtensionCodersFactoryProtocol
    public let eventCenter: EventCenterProtocol
    public let operationQueue: OperationQueue
    public let logger: SDKLoggerProtocol

    public init(
        fileOperationFactory: RuntimeFilesOperationFactoryProtocol,
        repository: AnyDataProviderRepository<RuntimeMetadataItem>,
        dataOperationFactory: DataOperationFactoryProtocol,
        signedExtensionCodersFactory: SignedExtensionCodersFactoryProtocol,
        eventCenter: EventCenterProtocol,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.fileOperationFactory = fileOperationFactory
        self.repository = repository
        self.dataOperationFactory = dataOperationFactory
        self.signedExtensionCodersFactory = signedExtensionCodersFactory
        self.eventCenter = eventCenter
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension RuntimeProviderFactory: RuntimeProviderFactoryProtocol {
    public func createRuntimeProvider(for chain: ChainModel) -> RuntimeProviderProtocol {
        let snapshotOperationFactory = RuntimeSnapshotFactory(
            chainId: chain.chainId,
            filesOperationFactory: fileOperationFactory,
            repository: repository,
            runtimeTypeRegistryFactory: RuntimeTypeRegistryFactory(
                signedExtensionCodersFactory: signedExtensionCodersFactory,
                logger: logger
            )
        )

        return RuntimeProvider(
            chainModel: chain,
            snapshotOperationFactory: snapshotOperationFactory,
            eventCenter: eventCenter,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
