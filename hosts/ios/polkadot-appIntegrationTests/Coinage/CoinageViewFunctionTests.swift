import Testing
import Foundation
import SubstrateSdk
import SubstrateOperation
import SDKLogger
import Coinage
import BigInt
@testable import polkadot_app

struct CoinageViewFunctionTests {
    @Test func fetchFreeUnloadTokenInfo() async throws {
        let logger = Logger.shared
        let substrateStorageFacade = SubstrateStorageTestFacade()

        FirebaseFacade.shared.fetchRemoteConfigValues()

        let chainRegistry = ChainRegistryFacade.setupForIntegrationTest(
            with: substrateStorageFacade,
            logger: logger
        )

        let executor = ViewFunctionExecutor(chainRegistry: chainRegistry, operationQueue: OperationQueue())

        let info: FreeUnloadTokenInfo = try await executor.call(
            viewFunction: ViewFunctionCodingPath(moduleName: "Coinage", functionName: "get_free_unload_token_info"),
            chainId: KnownChainId.previewNetPeople
        )

        logger.debug(
            "Free unload token limits — people: \(String(describing: info.people)), " +
                "lite people: \(String(describing: info.litePeople))"
        )
    }

    @Test func getPaidTokenRingMembers() async throws {
        let logger = Logger.shared
        let substrateStorageFacade = SubstrateStorageTestFacade()

        FirebaseFacade.shared.fetchRemoteConfigValues()

        let chainRegistry = ChainRegistryFacade.setupForIntegrationTest(
            with: substrateStorageFacade,
            logger: logger
        )

        let executor = ViewFunctionExecutor(chainRegistry: chainRegistry, operationQueue: OperationQueue())

        let members: [BytesCodable] = try await executor.call(
            viewFunction: ViewFunctionCodingPath(moduleName: "Coinage", functionName: "get_paid_token_ring_members"),
            chainId: KnownChainId.previewNetPeople,
            args: [UInt32(0), UInt32(0)]
        )

        logger.debug("Members count: \(members.count)")
    }
}
