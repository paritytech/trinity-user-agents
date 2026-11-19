import Foundation
import Keystore_iOS
import ExtrinsicService
import ChainRegistry

extension ExtrinsicOriginFactory {
    static func createSigned() -> ExtrinsicOriginDefiningFactoryProtocol {
        SignedExtrinsicOriginFactory(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: Logger.shared
        )
    }

    static func lightPerson() -> ExtrinsicOriginDefiningFactoryProtocol {
        PersonLiteOriginFactory(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: Logger.shared
        )
    }

    static func `default`() -> ExtrinsicOriginFactoryProtocol {
        ExtrinsicOriginFactory(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: Logger.shared
        )
    }
}
