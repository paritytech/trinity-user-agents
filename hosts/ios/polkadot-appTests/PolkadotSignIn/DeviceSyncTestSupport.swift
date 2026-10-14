@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

extension DeviceSyncExchangeTests {
    func makeSession(
        peerAccountId: Data,
        flow: MockDeviceSyncPeerConnectionFlow,
        failureRecorder: DeviceSyncFailureRecorder? = nil,
        remoteContactResolver: RemoteContactResolving = MockRemoteContactResolver(result: .success(nil)),
        deviceRepositoryFactory: LocalDeviceRepositoryMaking = MockLocalDeviceRepositoryFactory(),
        contactRepositoryFactory: ChatContactRepositoryMaking = MockChatContactRepositoryFactory(),
        messageRepositoryFactory: ChatMessageRepositoryMaking = MockChatMessageRepositoryFactory(),
        removedChatRepositoryFactory: RemovedChatRepositoryMaking = MockRemovedChatRepositoryFactory(),
        outgoingUpdateTimeRepositoryFactory: OutgoingUpdateTimeRepositoryMaking =
            MockOutgoingUpdateTimeRepositoryFactory(),
        ackTimeout: Duration = .seconds(3_600),
        sleep: @escaping DeviceSyncSleep = liveDeviceSyncSleep,
        entityChunker: DeviceSyncEntityChunker = .init()
    ) -> DeviceSyncExchange {
        let messageExchangeModeProvider = ChatMessageExchangeModeProvider(tld: "dot")
        let storageFacade = UnusedDeviceSyncStorageFacade()

        return DeviceSyncExchange(
            peer: .init(statementAccountId: peerAccountId, initialCheckpoint: nil),
            flow: flow,
            dependencies: .init(
                remoteContactResolver: remoteContactResolver,
                deviceRepositoryFactory: deviceRepositoryFactory,
                contactRepositoryFactory: contactRepositoryFactory,
                chatRepositoryFactory: MockChatRepositoryFactory(),
                messageRepositoryFactory: messageRepositoryFactory,
                removedChatRepositoryFactory: removedChatRepositoryFactory,
                outgoingUpdateTimeRepositoryFactory: outgoingUpdateTimeRepositoryFactory,
                contactDataProviderFactory: MockChatContactDataProviderFactory(),
                messageDataProviderFactory: MockChatMessageDataProviderFactory(),
                messageExchangeModeProvider: messageExchangeModeProvider,
                contactsStorageService: ContactsLocalStorageService(
                    storageFacade: storageFacade,
                    logger: MockLogger()
                ),
                storageFacade: storageFacade,
                updateIdProvider: MockDeviceSyncUpdateIdProvider()
            ),
            peerLogger: MockLogger(),
            behavior: .init(
                ackTimeout: ackTimeout,
                sleep: sleep,
                entityChunker: entityChunker
            ),
            handlers: .init(
                failure: { failure in
                    await failureRecorder?.record(accountId: peerAccountId, failure: failure)
                }
            )
        )
    }
}

private final class UnusedDeviceSyncStorageFacade: StorageFacadeProtocol {
    var databaseService: CoreDataServiceProtocol {
        fatalError("Storage is not used by these device sync tests")
    }

    func createRepository<T, U>(
        filter _: NSPredicate?,
        sortDescriptors _: [NSSortDescriptor],
        mapper _: AnyCoreDataMapper<T, U>
    ) -> CoreDataRepository<T, U> where T: Identifiable, U: NSManagedObject {
        fatalError("Storage is not used by these device sync tests")
    }
}
