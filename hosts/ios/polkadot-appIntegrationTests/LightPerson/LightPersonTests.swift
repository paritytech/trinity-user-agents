import SubstrateSdkExt
@testable import polkadot_app
import XCTest
import BigInt
import Keystore_iOS
import NovaCrypto
import SubstrateSdk
import Operation_iOS
import ExtrinsicService
import XcmDefinition
import SDKLogger
import KeyDerivation
import Individuality
import AssetsManagement
import ChainRegistry

final class LightPersonTests: XCTestCase {
    let mnemonic = "admit bounce found rally person winner script thing supreme honey credit goddess"
    let username = "scorpiontest"

    let verifierAddress = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"

    let assetId = Xcm.Version4(
        wrapped: XcmUni.AssetId(
            location: .init(
                parents: 1,
                items: [
                    XcmUni.Junction.parachain(1_000),
                    XcmUni.Junction.palletInstance(50),
                    XcmUni.Junction.generalIndex(3)
                ]
            )
        )
    )

    func testGenerateAttestationParams() throws {
        let logger: SDKLoggerProtocol = Logger.shared
        do {
            let verifier = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"
            let verifierAddress = try verifier.toAccountId()
            let setupResult = try setupLocalMetaAccountModel()
            let factory = try LitePersonParamsFactory(
                mainWallet: setupResult.main,
                liteVrfManager: BandersnatchKeyManager(
                    entropyDeriver: RingVrfEntropyDeriver(domain: BuiltInProduct.personhood(for: "dot"), index: 1),
                    entropyManager: setupResult.entropyManager
                ),
                chatEncryptorManager: ChatEncryptionManager(
                    entropyManager: setupResult.entropyManager
                )
            )

            let params = try factory.deriveLitePersonParams(
                for: username,
                verifier: verifierAddress,
                reservedUsername: nil
            )

            let mainAddress = try params.accountId.toAddress(using: .substrate(42))
            logger.info("main: \(mainAddress)")
            logger.info("main signature (SR25519): \(params.accountIdProofSignature.toHex(includePrefix: true))")
            logger.info("ringVrfKey: \(params.personMemberKey.toHex(includePrefix: true))")
            logger.info("proofOfOwnership: \(params.membershipProofSignature.toHex(includePrefix: true))")
            logger.info(
                "consumerRegistration.signature (SR25519): \(params.resourcesSignature.toHex(includePrefix: true))"
            )
            let identifierKey = try params.encryptionIdentifier.scaleEncoded().toHex(includePrefix: true)
            logger.info("consumerRegistration.identifierKey: \(identifierKey)")
            logger.info("username: \(params.username)")
        } catch {
            logger.error("Unexpected error: \(error)")
        }
    }

    func testFreeTransfer() throws {
        let logger: SDKLoggerProtocol = Logger.shared
        let operationQueue = OperationQueue()
        let substrateStorageFacade = SubstrateStorageTestFacade()

        do {
            let setupResult = try setupLocalMetaAccountModel()

            let asset = try assetId.toScaleCompatibleJSON(with: nil)

            let chainRegistry = ChainRegistryFacade.setupForIntegrationTest(
                with: substrateStorageFacade,
                logger: Logger.shared
            )

            let chain = try chainRegistry.getChainOrError(for: KnownChainId.previewNetPeople)

            let extrinsicMonitor = try ExtrinsicSubmissionMonitorFacade(
                chainRegistry: chainRegistry,
                substrateStorageFacade: substrateStorageFacade,
                operationQueue: operationQueue
            ).createMonitorFactory(
                chain: chain
            )

            let origin = try PersonLiteOriginFactory(
                chainRegistry: chainRegistry,
                operationQueue: operationQueue,
                logger: Logger.shared
            ).extrinsicOriginDefiner(
                from: setupResult.main,
                chain: chain
            )

            let reciepient = try verifierAddress.toAccountId()

            let wrapper = extrinsicMonitor.submitAndMonitorWrapper(
                extrinsicBuilderClosure: { builder in
                    let call = AssetsPallet.Transfer(
                        assetId: asset,
                        target: .accoundId(reciepient),
                        amount: Decimal(0.1).toSubstrateAmount(precision: 18)!
                    )

                    let dispatchCall = PeopleLitePallet.DispatchAsSignerCall(
                        call: call.runtimeCall()
                    )

                    return try builder.adding(call: dispatchCall.runtimeCall())
                },
                origin: origin,
                params: ExtrinsicSubmissionParams(
                    feeAssetId: nil,
                    eventsMatcher: nil
                )
            )

            operationQueue.addOperations(wrapper.allOperations, waitUntilFinished: true)

            let result = try wrapper.targetOperation.extractNoCancellableResultData()

            logger.debug("Result: \(result)")
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }
}

private extension LightPersonTests {
    struct SetupParams {
        let main: WalletManaging
        let storageFacade: StorageFacadeProtocol
        let entropyManager: RootEntropyManaging
    }

    func setupLocalMetaAccountModel() throws -> SetupParams {
        let userStorageFacade = UserDataStorageTestFacade()
        let keychain = InMemoryKeychain()

        let entropyManager = RootEntropyManager(
            keychain: keychain,

            installationKeyIdStore: InstallationKeyIdStore(userDefaults: .standard)
        )

        let manager = WalletSetupManager(
            mnemonicGenerator: IRMnemonicCreator(),
            mnemonicBackupHelper: MockMnemonicBackupHelper(),
            entropyManager: entropyManager,
            logger: Logger.shared
        )

        let mnemonic = try IRMnemonicCreator().mnemonic(fromList: mnemonic)
        try manager.createWallets(with: .init(mnemonic: mnemonic))

        return SetupParams(
            main: DynamicDerivedWallet(
                derivationPath: WalletDerivationPath.main(for: "dot"),
                entropyManager: entropyManager
            ),
            storageFacade: userStorageFacade,
            entropyManager: entropyManager
        )
    }
}
