import XCTest
import ChainRegistry
@testable import polkadot_app
import SubstrateSdk
import XcmTransfer
import Operation_iOS

final class XcmSyncTests: XCTestCase {
    func testSyncXcmTransfers() throws {
        let operationQueue = OperationQueue()
        let logger = Logger.shared
        let storageFacade = SubstrateStorageTestFacade()

        let chainRegistry = ChainRegistryFacade.setupForIntegrationTest(with: storageFacade)
        let syncService = XcmTransfersSyncService(
            remoteConfigManager: FirebaseFacade.shared,
            operationQueue: operationQueue,
            logger: logger
        )

        var xcmConfig: XcmTransfers?

        let expectation = XCTestExpectation()

        syncService.notificationCallback = { result in
            xcmConfig = try? result.get()
            expectation.fulfill()
        }

        syncService.setup()

        wait(for: [expectation], timeout: 10)

        guard let xcmConfig else {
            XCTFail("No config")
            return
        }

        let pah = try chainRegistry.getChainOrError(for: KnownChainId.polkadotAH)
        let hydration = try chainRegistry.getChainOrError(for: KnownChainId.hydration)

        guard let ahUSDT = pah.chainAssetInterfaceForSymbol("USDT") else {
            XCTFail("No USDT on \(pah.name)")
            return
        }

        guard let hydrationUSDT = hydration.chainAssetInterfaceForSymbol("USDT") else {
            XCTFail("No USDT on \(hydration.name)")
            return
        }

        let ahAvailableDest = xcmConfig.getDestinations(for: ahUSDT.chainAssetId)
        XCTAssertTrue(ahAvailableDest.contains(hydrationUSDT.chainAssetId), "Can send USDT to hydration")

        let people = try chainRegistry.getChainOrError(for: KnownChainId.polkadotPeople)

        guard let peopleUSDT = people.chainAssetInterfaceForSymbol("USDT") else {
            XCTFail("No USDT on \(people.name)")
            return
        }

        let hydraAvailableDest = xcmConfig.getDestinations(for: hydrationUSDT.chainAssetId)
        XCTAssertTrue(hydraAvailableDest.contains(peopleUSDT.chainAssetId), "Can send USDT to people")
    }
}
