@testable import polkadot_app
import Foundation
import MessageExchangeKit
import StatementStore
import SubstrateSdk
import Testing

@Suite(.timeLimit(.minutes(1)))
struct DeviceSyncServiceTests {
    @Test("Accepted offer id is persisted by the service")
    func acceptedOfferIdIsPersisted() async throws {
        let peerAccountId = Data(repeating: 2, count: 32)
        let repositoryFactory = MockLastSyncOfferIdRepositoryFactory()
        let service = DeviceSyncService(
            ownStatementAccountId: Data(repeating: 1, count: 32),
            messageExchangeModeProvider: FixedMessageExchangeModeProvider(mode: .identity),
            configFactory: MockDeviceSyncWebRTCConfigFactory(),
            remoteContactResolver: MockRemoteContactResolver(result: .success(nil)),
            storageDependencies: .init(
                deviceDataProviderFactory: EmptyLocalDeviceDataProviderFactory(),
                deviceRepositoryFactory: MockLocalDeviceRepositoryFactory(),
                contactRepositoryFactory: MockChatContactRepositoryFactory(),
                chatRepositoryFactory: MockChatRepositoryFactory(),
                messageRepositoryFactory: MockChatMessageRepositoryFactory(),
                removedChatRepositoryFactory: MockRemovedChatRepositoryFactory(),
                lastSyncOfferIdRepositoryFactory: repositoryFactory
            ),
            logger: MockLogger()
        )
        let persistenceHandler = await service.makeOfferIdPersistenceHandler(for: peerAccountId)

        let sourceTransport = MockDeviceSyncMessageTransport()
        let initiator = DeviceSyncSignalingSession(
            transport: sourceTransport,
            role: .initiator,
            peerLogger: MockLogger()
        )
        try await initiator.send([.offer(SdpCoderTests.validOfferSdp)])
        let encodedOffer = try #require(sourceTransport.sentMessages.first)
        let offer = try Chat.DeviceSyncSignalingEnvelope(
            scaleDecoder: ScaleDecoder(data: encodedOffer)
        )

        let acceptor = DeviceSyncSignalingSession(
            transport: MockDeviceSyncMessageTransport(),
            role: .acceptor,
            offerIdHandler: persistenceHandler,
            peerLogger: MockLogger()
        )
        await acceptor.handleIncomingEnvelopes([offer])

        let savedUpdate = await repositoryFactory.waitForSavedUpdate()
        #expect(savedUpdate.statementAccountId == peerAccountId)
        #expect(savedUpdate.lastSyncOfferId == offer.offerId)
        #expect(await acceptor.currentOfferId() == offer.offerId)
    }

    @Test("Device snapshots reconcile peer engine membership")
    func deviceSnapshotsReconcilePeerEngines() async {
        let peerAccountId = Data(repeating: 2, count: 32)
        let devices = ControllableLocalDeviceDataProviderFactory()
        let engine = MockDeviceSyncPeerEngine()
        let service = DeviceSyncService(
            ownStatementAccountId: Data(repeating: 1, count: 32),
            messageExchangeModeProvider: FixedMessageExchangeModeProvider(mode: .identity),
            configFactory: MockDeviceSyncWebRTCConfigFactory(),
            storageDependencies: .init(deviceDataProviderFactory: devices),
            peerEngineFactory: { _, _ in engine }
        )
        await service.setup(configuration: makeConfiguration())

        devices.send([makeDevice(accountId: peerAccountId)])
        await engine.waitUntilStarted()
        devices.send([])
        await engine.waitUntilDisposalStarts()

        #expect(await engine.isDisposed)
        await service.throttle()
    }

    @Test("Throttle during a device update cannot strand a peer engine")
    func throttleDuringDevicesUpdateDisposesEveryEngine() async {
        let firstPeer = Data(repeating: 2, count: 32)
        let secondPeer = Data(repeating: 3, count: 32)
        let devices = ControllableLocalDeviceDataProviderFactory()
        let disposalGate = DeviceSyncCancellationGate()
        let firstEngine = MockDeviceSyncPeerEngine(disposalGate: disposalGate)
        let secondEngine = MockDeviceSyncPeerEngine()
        let service = DeviceSyncService(
            ownStatementAccountId: Data(repeating: 1, count: 32),
            messageExchangeModeProvider: FixedMessageExchangeModeProvider(mode: .identity),
            configFactory: MockDeviceSyncWebRTCConfigFactory(),
            storageDependencies: .init(deviceDataProviderFactory: devices),
            peerEngineFactory: { peerAccountId, _ in
                peerAccountId == firstPeer ? firstEngine : secondEngine
            }
        )
        await service.setup(configuration: makeConfiguration())

        devices.send([makeDevice(accountId: firstPeer)])
        await firstEngine.waitUntilStarted()
        devices.send([makeDevice(accountId: secondPeer)])
        await secondEngine.waitUntilStarted()
        await firstEngine.waitUntilDisposalStarts()

        await service.throttle()

        #expect(await firstEngine.isDisposed)
        #expect(await secondEngine.isDisposed)
    }
}

private extension DeviceSyncServiceTests {
    func makeConfiguration() -> DeviceSyncServiceConfiguration {
        DeviceSyncServiceConfiguration(
            connection: MockStatementStore(),
            signerManager: ClosureSignerManager { _ in fatalError("Signer is not used") },
            encryptionManager: ClosureEncryptionManager { _ in fatalError("Encryption is not used") },
            ownSignKeyId: "sign-key",
            ownEncryptionKeyId: "encryption-key"
        )
    }

    func makeDevice(accountId: Data) -> Chat.LocalDevice {
        Chat.LocalDevice(
            statementAccountId: accountId,
            encryptionPublicKey: Data(repeating: 4, count: 65),
            hostName: "Polkadot Desktop",
            createdAt: Date(),
            hostVersion: nil,
            osType: nil,
            osVersion: nil,
            outgoingUpdateTime: nil,
            lastSyncOfferId: nil
        )
    }
}
