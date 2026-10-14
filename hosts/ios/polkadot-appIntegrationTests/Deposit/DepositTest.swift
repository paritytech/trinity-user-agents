import XCTest
import ChainRegistry
@testable import polkadot_app
import SDKLogger
import Operation_iOS
import SubstrateSdk
import Keystore_iOS
import CommonService
import AssetExchange
import NovaCrypto
import BigInt
import KeyDerivation

enum DepositTestError: Error {
    case unexpectedPublicKey
}

final class DepositTest: XCTestCase {
    struct WalletSetup {
        let accountToFund: AccountId
        let depositWallet: WalletManaging
    }

    let mainWallet = "can alien pipe split prosper require trade oxygen observe siren note surround"

    func testDetectSwapPath() throws {
        // given

        let walletSetup = try setupWallet()
        let storageFacade = SubstrateStorageTestFacade()
        let chainRegistry = ChainRegistryFacade.setupForIntegrationTest(with: storageFacade)
        let operationQueue = OperationQueue()

        let pah = try chainRegistry.getChainOrError(for: KnownChainId.polkadotAH)

        guard let usdtPah = pah.chainAssetInterfaceForSymbol("USDT") else {
            XCTFail("No USDT on \(pah.name)")
            return
        }

        let people = try chainRegistry.getChainOrError(for: KnownChainId.polkadotPeople)

        guard let usdtPeople = people.chainAssetInterfaceForSymbol("USDT") else {
            XCTFail("No USDT on \(people.name)")
            return
        }

        // setup exchange service
        let setupModel = try AssetExchangeServiceFactory(
            depositWallet: walletSetup.depositWallet,
            accountToFund: walletSetup.accountToFund,
            fundedAssetId: usdtPeople.chainAssetId,
            hydrationChainId: KnownChainId.hydration,
            ahChainId: KnownChainId.polkadotAH,
            usdtChainId: KnownChainId.polkadotAH,
            feePercentageBuffer: BigRational.percent(of: 10),
            chainRegistry: chainRegistry,
            storageFacade: storageFacade,
            exchangesStateMediator: AssetsExchangeStateMediator(),
            priceStore: AssetExchangePriceStore(
                priceLocalSubscriptionFactory: PriceProviderFactory.shared,
                chainRegistry: chainRegistry,
                logger: Logger.shared
            ),
            configManager: FirebaseFacade.shared,
            operationQueue: operationQueue,
            logger: Logger.shared
        ).createService()

        setupModel.service.setup()

        // give some time to setup
        let expectation = XCTestExpectation()

        DispatchQueue.global().asyncAfter(deadline: .now() + .seconds(10)) {
            expectation.fulfill()
        }

        wait(for: [expectation], timeout: 60)

        // estimate fees

        let amount = Decimal(10).toSubstrateAmount(precision: usdtPah.assetInterface.decimalPrecision)!

        let quoteWrapper = setupModel.service.fetchQuoteWrapper(
            for: AssetConversion.QuoteArgs(
                assetIn: usdtPah.chainAssetId,
                assetOut: usdtPeople.chainAssetId,
                amount: amount,
                direction: .sell
            )
        )

        operationQueue.addOperations(quoteWrapper.allOperations, waitUntilFinished: true)

        let quote = try quoteWrapper.targetOperation.extractNoCancellableResultData()

        Logger.shared.debug("Quote: \(quote)")

        let feeWrapper = setupModel.service.estimateFee(
            for: AssetExchangeFeeArgs(
                route: quote.route,
                slippage: BigRational(numerator: 5, denominator: 1_000),
                feeAssetId: usdtPah.chainAssetId,
                destinationAccountId: walletSetup.accountToFund
            )
        )

        operationQueue.addOperations(feeWrapper.allOperations, waitUntilFinished: true)

        let fee = try feeWrapper.targetOperation.extractNoCancellableResultData()

        Logger.shared.debug("Fee: \(fee)")
    }
}

private extension DepositTest {
    func setupWallet() throws -> WalletSetup {
        let keystore = InMemoryKeychain()

        let mnemonic = try IRMnemonicCreator().mnemonic(fromList: mainWallet)
        let entropy = mnemonic.entropy()

        let entropyManager = RootEntropyManager(
            keychain: keystore,

            installationKeyIdStore: InstallationKeyIdStore(userDefaults: .standard)
        )
        try entropyManager.createRootEntropy(entropy)

        let wallet = DynamicDerivedWallet(derivationPath: "//deposit", entropyManager: entropyManager)
        let accountToFund = try wallet.getRawPublicKey()

        return WalletSetup(accountToFund: accountToFund, depositWallet: wallet)
    }
}
