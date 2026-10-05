import Foundation
import UIKit
import Keystore_iOS
import KeyDerivation
import SubstrateOperation
import ChainRegistry

extension ChatExtensionsRegistry {
    @MainActor
    static func createDimExtensions(
        syncStateStore: DetermineStateSyncStore,
        personDataStore: DetermineStatePersonDataStore,
        syncService: DetermineStateSyncServicing,
        personhoodRegistrationService: PersonhoodRegistrationServicing,
        audioSessionManager: AudioSessionManaging
    ) -> [ChatExtending] {
        #if !FEATURE_DIMS
            return []
        #else
            do {
                let logger = Logger.shared

                let dimsState = try createFlowStates(
                    syncStateStore: syncStateStore,
                    personDataStore: personDataStore,
                    syncService: syncService,
                    personhoodRegistrationService: personhoodRegistrationService
                )
                let settings = SettingsManager.shared

                #if FEATURE_DIMS_FULL
                    let dim2Actions: [ChatExtensionActions.ActionModel] = [
                        .init(
                            title: String(localized: .MobRule.chatName),
                            subtitle: String(localized: .ChatExtension.actionOpenMobRulesSubtitle),
                            identifier: MobRulesChatExtension.identifier
                        )
                    ]
                #else
                    let dim2Actions: [ChatExtensionActions.ActionModel] = []
                #endif

                guard let dim2 = WeeklyGameFactory.create(
                    settings: settings,
                    dependencies: DIM2Dependencies(
                        dim2SharedState: dimsState.dim2,
                        dimsSharedState: dimsState.peerState
                    ),
                    personActions: dim2Actions
                ) else {
                    logger.error("Can't create dim2")

                    return []
                }

                #if FEATURE_DIMS_FULL
                    guard let mobRules = MobRulesFactory.create(
                        settings: settings,
                        scoreInfoSyncService: dimsState.peerState.scoreInfoSyncService
                    ) else {
                        logger.error("Can't create mob rule")
                        return []
                    }

                    let dim1 = createDIM1(
                        flowState: dimsState.dim1,
                        audioSessionManager: audioSessionManager
                    )

                    let peerActions: [ChatExtensionActions.ActionModel] = [
                        .init(
                            title: String(localized: .ChatExtension.polkadotPeerActionDim1Title),
                            subtitle: String(localized: .ChatExtension.polkadotPeerActionDim1Subtitle),
                            identifier: dim1.identifier
                        ),
                        .init(
                            title: String(localized: .ChatExtension.polkadotPeerActionDim2Title),
                            subtitle: String(localized: .ChatExtension.polkadotPeerActionDim2Subtitle),
                            identifier: DIM2ChatExtension.identifier
                        )
                    ]

                    let peerChat = createPolkadotPeer(
                        flowState: dimsState.peerState,
                        actions: peerActions,
                        logger: logger
                    )

                    return [peerChat, dim1, dim2, mobRules]
                        .compactMap { $0 }
                #else
                    return [dim2]
                #endif
            } catch {
                Logger.shared.error("Can't create dims state: \(error)")
                return []
            }
        #endif
    }
}

private extension ChatExtensionsRegistry {
    @MainActor
    static func createPolkadotPeer(
        flowState: DIMSSharedFlowStateProtocol,
        actions: [ChatExtensionActions.ActionModel],
        logger: LoggerProtocol
    ) -> PolkadotPeer? {
        PolkadotPeer(
            actions: actions,
            wireframe: PolkadotPeerWireframe(application: UIApplication.shared),
            interactor: PolkadotPeerInteractor(flowState: flowState),
            logger: logger
        )
    }

    #if FEATURE_DIMS_FULL
        @MainActor
        static func createDIM1(
            flowState: DIM1SharedFlowStateProtocol,
            audioSessionManager: AudioSessionManaging
        ) -> DIM1ChatExtension {
            let videoPreviewPlayerFactory = VideoPreviewPlayerFactory(
                audioSessionManager: audioSessionManager
            )

            let wireframe = DIM1Wireframe(
                application: UIApplication.shared,
                botSettings: SettingsManager.shared,
                videoPreviewPlayerFactory: videoPreviewPlayerFactory
            )

            let interactor = DIM1ChatInteractor(
                flowState: flowState,
                notificationService: DIM1NotificationService(
                    localNotificationService: UserNotificationService.shared
                )
            )

            let actions: [ChatExtensionActions.ActionModel] = [
                .init(
                    title: String(localized: .MobRule.chatName),
                    subtitle: String(localized: .ChatExtension.actionOpenMobRulesSubtitle),
                    identifier: MobRulesChatExtension.identifier
                ),
                .init(
                    title: String(localized: .WeeklyGame.chatName),
                    subtitle: String(localized: .ChatExtension.actionOpenWeeklyGameSubtitle),
                    identifier: DIM2ChatExtension.identifier
                )
            ]

            return DIM1ChatExtension(
                interactor: interactor,
                wireframe: wireframe,
                personActions: actions
            )
        }
    #endif

    struct DimStates {
        let peerState: DIMSSharedFlowStateProtocol
        let dim1: DIM1SharedFlowStateProtocol
        let dim2: DIM2SharedFlowStateProtocol
    }

    static func createFlowStates(
        syncStateStore: DetermineStateSyncStore,
        personDataStore: DetermineStatePersonDataStore,
        syncService: DetermineStateSyncServicing,
        personhoodRegistrationService: PersonhoodRegistrationServicing
    ) throws -> DimStates {
        let peerFlowState = try createDIMSFlowState(
            syncStateStore: syncStateStore,
            personDataStore: personDataStore,
            syncService: syncService,
            personhoodRegistrationService: personhoodRegistrationService
        )
        let dim1FlowState = try createDIM1FlowState(for: peerFlowState)
        let dim2FlowState = try createDIM2FlowState(for: peerFlowState)

        return DimStates(peerState: peerFlowState, dim1: dim1FlowState, dim2: dim2FlowState)
    }

    private static func createDIMSFlowState(
        syncStateStore: DetermineStateSyncStore,
        personDataStore: DetermineStatePersonDataStore,
        syncService: DetermineStateSyncServicing,
        personhoodRegistrationService: PersonhoodRegistrationServicing
    ) throws -> DIMSSharedFlowState {
        let walletRepo: WalletManagerRepositoryProtocol = .shared
        let vrfRepo: BandersnatchManagerRepositoryProtocol = .shared
        let candidateAccountId = try walletRepo.candidate().getRawPublicKey()
        let mobRuleAccountId = try walletRepo.mobRuleAlias().getRawPublicKey()
        let scoreAccountId = try walletRepo.scoreAlias().getRawPublicKey()
        let resourcesAccountId = try walletRepo.resourcesAlias().getRawPublicKey()
        let vrfManager = try vrfRepo.fullPerson()
        let memberKey = try vrfManager.getMemberKey()
        let chainRegistry = ChainRegistryFacade.sharedRegistry

        let syncQueue = DispatchQueue(label: "io.polkadot.app.dims.service.queue")

        let chain = try chainRegistry.getChainOrError(for: AppConfig.Chains.usernameChain)
        let connection = try chainRegistry.getConnectionOrError(for: chain.chainId)
        let runtimeService = try chainRegistry.getRuntimeProviderOrError(for: chain.chainId)

        let personRegistrationSyncService = PersonhoodRegistrationSyncService(
            candidateAccountId: candidateAccountId,
            mobRuleAccountId: mobRuleAccountId,
            scoreAccountId: scoreAccountId,
            resourcesAccountId: resourcesAccountId,
            memberKey: memberKey,
            connection: connection,
            runtimeService: runtimeService,
            observers: [personhoodRegistrationService],
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            proccessingQueue: syncQueue,
            logger: Logger.shared
        )

        let gameInfoSyncService = GameInfoSyncService(
            accountOrPerson: nil,
            connection: connection,
            runtimeService: runtimeService,
            observer: syncStateStore
        )

        let scoreInfoSyncService = ScoreInfoSyncService(
            accountOrPerson: nil,
            connection: connection,
            runtimeService: runtimeService,
            observer: syncStateStore
        )

        // Cast required: DIMSSharedFlowState init expects concrete DetermineStateSyncService,
        // but we receive the protocol. ServiceCoordinator always creates the concrete type.
        guard let concreteSyncService = syncService as? DetermineStateSyncService else {
            throw DIMSFlowStateError.invalidSyncServiceType
        }

        return DIMSSharedFlowState(
            syncService: concreteSyncService,
            syncStateStore: syncStateStore,
            personDataStore: personDataStore,
            personhoodRegistrationService: personhoodRegistrationService,
            personRegistrationSyncService: personRegistrationSyncService,
            gameInfoSyncService: gameInfoSyncService,
            scoreInfoSyncService: scoreInfoSyncService
        )
    }

    enum DIMSFlowStateError: Error {
        case invalidSyncServiceType
    }

    private static func createDIM2FlowState(for peerState: DIMSSharedFlowState) throws -> DIM2SharedFlowState {
        let chatEncryptionManager = try ChatEncryptionManager().makeEncryptorFactory(
            ownEncryptionKeyId: Chat.Contact.Own.gameEncryptionKeyId()
        )

        let settingsManager = SettingsManager.shared
        let gameStartReminder: any GameStartReminderServicing =
            if #available(iOS 26.1, *) {
                AlarmKitGameReminder(
                    alarmManger: .shared,
                    settingsManager: settingsManager
                )
            } else {
                LocalNotificationGameReminder(
                    localNotificationService: UserNotificationService.shared,
                    settingsManager: settingsManager
                )
            }

        let walletRepo: WalletManagerRepositoryProtocol = .shared
        let vrfRepo: BandersnatchManagerRepositoryProtocol = .shared

        return try DIM2SharedFlowState(
            candidateWallet: walletRepo.candidate(),
            score: walletRepo.scoreAlias(),
            vrfManager: vrfRepo.fullPerson(),
            chatEncryptionManager: chatEncryptionManager,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            chainId: AppConfig.Chains.usernameChain,
            personDataStore: peerState.personDataStore,
            commonStateStore: peerState.syncStateStore,
            gameInfoSyncService: peerState.gameInfoSyncService,
            scoreInfoSyncService: peerState.scoreInfoSyncService,
            substrateStorageFacade: SubstrateDataStorageFacade.shared,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            gameStartReminder: gameStartReminder,
            logger: Logger.shared
        )
    }

    private static func createDIM1FlowState(for peerState: DIMSSharedFlowState) throws -> DIM1SharedFlowState {
        let walletRepo: WalletManagerRepositoryProtocol = .shared
        let vrfRepo: BandersnatchManagerRepositoryProtocol = .shared

        return try DIM1SharedFlowState(
            commonStateStore: peerState.syncStateStore,
            personStateStore: peerState.personDataStore,
            gameInfoSyncService: peerState.gameInfoSyncService,
            vrfManager: vrfRepo.fullPerson(),
            candidateWallet: walletRepo.candidate(),
            mobRuleWallet: walletRepo.mobRuleAlias(),
            scoreWallet: walletRepo.scoreAlias(),
            resourcesWallet: walletRepo.resourcesAlias(),
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            proofOfInkChainId: AppConfig.Chains.usernameChain,
            gameChainId: AppConfig.Chains.usernameChain,
            userStorageFacade: UserDataStorageFacade.shared,
            substrateStorageFacade: SubstrateDataStorageFacade.shared,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            fileManager: .default,
            logger: Logger.shared
        )
    }
}
