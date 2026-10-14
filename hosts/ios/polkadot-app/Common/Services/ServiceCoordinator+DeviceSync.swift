import Foundation
import CryptoKit
import StatementStore
import ChainRegistry

extension ServiceCoordinator {
    static func createDeviceSyncService(
        turnService: TURNCredentialsProviding,
        logger: LoggerProtocol
    ) throws -> DeviceSyncServicing {
        do {
            let signerManager = ChatSignerManager()
            let signer = try signerManager.makeSigner(for: Chat.Contact.Own.main().signKeyId)
            let messageExchangeModeProvider = try ChatMessageExchangeModeProvider(
                tld: DotNsTldProviderFacade.shared.currentTldOrError()
            )

            return DeviceSyncService(
                ownStatementAccountId: signer.accountId,
                messageExchangeModeProvider: messageExchangeModeProvider,
                configFactory: WebRTCConfigFactory(turnService: turnService),
                logger: logger
            )
        } catch {
            logger.error("Device sync service creation error: \(error)")
            throw error
        }
    }
}

// MARK: - Device Sync Setup

extension ServiceCoordinator {
    func setupDeviceSyncService() async {
        do {
            let chainRegistry = ChainRegistryFacade.sharedRegistry
            let connection = try chainRegistry.getConnectionOrError(for: AppConfig.Chains.chatChain)

            let statementsConnection = StatementStoreConnection(
                connection: connection,
                retryMatcher: StatementSubmitErrorMatcher.retryWhenTimeoutOrNoAllowance(),
                logger: logger
            )

            let encryptionKey = try DeviceEncryptionKeyManager.shared.getOrCreatePrivateKey()

            let ownKeyId = try Chat.Contact.Own.main()

            await deviceSyncService.setup(configuration: DeviceSyncServiceConfiguration(
                connection: statementsConnection,
                signerManager: ChatSignerManager(),
                encryptionManager: DeviceSyncEncryptionManager(privateKey: encryptionKey),
                ownSignKeyId: ownKeyId.signKeyId,
                ownEncryptionKeyId: ownKeyId.encryptionKeyId
            ))
        } catch {
            logger.error("Failed to setup DeviceSyncService: \(error)")
        }
    }
}
