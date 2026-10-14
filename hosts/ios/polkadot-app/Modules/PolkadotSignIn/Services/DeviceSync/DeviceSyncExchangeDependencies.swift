import Foundation
import MessageExchangeKit

struct DeviceSyncExchangePeer {
    let statementAccountId: Data
    let initialCheckpoint: UInt64?
}

struct DeviceSyncExchangeDependencies {
    let remoteContactResolver: RemoteContactResolving
    let deviceRepositoryFactory: LocalDeviceRepositoryMaking
    let contactRepositoryFactory: ChatContactRepositoryMaking
    let chatRepositoryFactory: ChatRepositoryMaking
    let messageRepositoryFactory: ChatMessageRepositoryMaking
    let removedChatRepositoryFactory: RemovedChatRepositoryMaking
    let outgoingUpdateTimeRepositoryFactory: OutgoingUpdateTimeRepositoryMaking
    let contactDataProviderFactory: ChatContactDataProviderMaking
    let messageDataProviderFactory: ChatMessageDataProviderMaking
    let messageExchangeModeProvider: any MessageExchangeModeProviding
    let contactsStorageService: ContactsLocalStorageServicing
    let storageFacade: StorageFacadeProtocol
    let updateIdProvider: DeviceSyncUpdateIdProviding
}

extension DeviceSyncExchangeDependencies {
    func makeIncomingUpdateApplier(
        flow: any DeviceSyncPeerConnectionFlowing,
        peerLogger: LoggerProtocol
    ) -> DeviceSyncIncomingUpdateApplier {
        DeviceSyncIncomingUpdateApplier(
            remoteContactResolver: remoteContactResolver,
            contactRepositoryFactory: contactRepositoryFactory,
            chatRepositoryFactory: chatRepositoryFactory,
            messageRepositoryFactory: messageRepositoryFactory,
            removedChatRepositoryFactory: removedChatRepositoryFactory,
            flow: flow,
            messageExchangeModeProvider: messageExchangeModeProvider,
            contactsStorageService: contactsStorageService,
            storageFacade: storageFacade,
            peerLogger: peerLogger
        )
    }

    func makeOutgoingChangesCollector() -> DeviceSyncOutgoingChangesCollector {
        DeviceSyncOutgoingChangesCollector(
            deviceRepositoryFactory: deviceRepositoryFactory,
            contactRepositoryFactory: contactRepositoryFactory,
            messageRepositoryFactory: messageRepositoryFactory,
            removedChatRepositoryFactory: removedChatRepositoryFactory,
            messageExchangeModeProvider: messageExchangeModeProvider
        )
    }
}

struct DeviceSyncExchangeBehavior {
    let ackTimeout: Duration
    let sleep: DeviceSyncSleep
    let entityChunker: DeviceSyncEntityChunker

    init(
        ackTimeout: Duration = DeviceSyncExchange.defaultAckTimeout,
        sleep: @escaping DeviceSyncSleep = liveDeviceSyncSleep,
        entityChunker: DeviceSyncEntityChunker = .init()
    ) {
        self.ackTimeout = ackTimeout
        self.sleep = sleep
        self.entityChunker = entityChunker
    }
}

struct DeviceSyncExchangeHandlers {
    let failure: @Sendable (DeviceSyncConnectionFailure) async -> Void

    init(
        failure: @escaping @Sendable (DeviceSyncConnectionFailure) async -> Void
    ) {
        self.failure = failure
    }
}
