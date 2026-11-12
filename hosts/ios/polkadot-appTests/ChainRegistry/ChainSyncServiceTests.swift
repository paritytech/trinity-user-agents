import Foundation
import Operation_iOS
import StructuredConcurrency
import Testing
import ChainRegistry
import EventCenter

@testable import polkadot_app

@Suite("ChainSyncService Tests", .timeLimit(.minutes(1)))
struct ChainSyncServiceTests {
    // MARK: - SUTModel

    struct SUTModel {
        let service: ChainSyncService
        let repository: AnyDataProviderRepository<ChainModel>
        let eventCenter: MockEventCenter
        // remoteConfig is unowned inside ChainSyncService, must be retained here
        let remoteConfig: MockRemoteConfigManager

        func fetchAll() async throws -> [ChainModel] {
            try await repository.fetchAllOperation(with: .init()).asyncExecute()
        }

        func awaitSyncComplete() async throws -> ChainSyncDidComplete {
            let event = await eventCenter.waitForEvent { $0 is ChainSyncDidComplete }

            return try #require(event as? ChainSyncDidComplete)
        }

        func awaitSyncEvent() async -> EventProtocol {
            await eventCenter.waitForEvent { $0 is ChainSyncDidComplete || $0 is ChainSyncDidFail }
        }
    }

    // MARK: - Helpers

    private func makeSUT(
        localChains: [ChainModel] = [],
        remoteChains: [RemoteChainModel]? = nil,
        remoteError: Error? = nil
    ) async throws -> SUTModel {
        let storageFacade = SubstrateStorageTestFacade()
        let mapper = ChainModelMapper()
        let chainRepository: CoreDataRepository<ChainModel, CDChain> =
            storageFacade.createRepository(mapper: AnyCoreDataMapper(mapper))
        let repository = AnyDataProviderRepository(chainRepository)

        let remoteConfig = MockRemoteConfigManager()
        remoteConfig.chainsToReturn = remoteChains ?? []
        remoteConfig.errorToThrow = remoteError

        let eventCenter = MockEventCenter()

        // Seed local chains
        if !localChains.isEmpty {
            try await repository.saveOperation({ localChains }, { [] }).asyncExecute()
        }

        let service = ChainSyncService(
            remoteConfigManager: remoteConfig,
            chainConverter: ChainModelConverter(),
            repository: repository,
            eventCenter: eventCenter,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: MockLogger()
        )

        return SUTModel(
            service: service,
            repository: repository,
            eventCenter: eventCenter,
            remoteConfig: remoteConfig
        )
    }

    // MARK: - Create (New Chains)

    @Test("Adds new chains when local is empty")
    func addsNewChainsWhenLocalIsEmpty() async throws {
        let remote1 = ChainMock.makeRemoteChain(name: "Polkadot")
        let remote2 = ChainMock.makeRemoteChain(name: "Kusama")

        let sut = try await makeSUT(remoteChains: [remote1, remote2])

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        #expect(result.newOrUpdatedChains.count == 2)

        let stored = try await sut.fetchAll()
        #expect(stored.contains { $0.chainId == remote1.chainId })
        #expect(stored.contains { $0.chainId == remote2.chainId })
    }

    @Test("Adds only new chain when one already exists locally")
    func addsOnlyNewChain() async throws {
        let remote1 = ChainMock.makeRemoteChain(name: "Polkadot")
        let remote2 = ChainMock.makeRemoteChain(name: "Kusama")
        let localChain = ChainMock.makeChainModel(from: remote1, order: 0)

        let sut = try await makeSUT(
            localChains: [localChain],
            remoteChains: [remote1, remote2]
        )

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        #expect(result.newOrUpdatedChains.contains { $0.chainId == remote2.chainId })

        let stored = try await sut.fetchAll()
        #expect(stored.count == 2)
    }

    // MARK: - Update Chains

    @Test("Updates chain when remote model changes")
    func updatesChain() async throws {
        let chainId = ChainMock.randomChainId()
        let remote = ChainMock.makeRemoteChain(id: chainId, name: "Polkadot")
        let localChain = ChainMock.makeChainModel(from: remote, order: 0)
        let updatedRemote = ChainMock.makeRemoteChain(id: chainId, name: "Polkadot Updated")

        let sut = try await makeSUT(
            localChains: [localChain],
            remoteChains: [updatedRemote]
        )

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        #expect(result.newOrUpdatedChains.count == 1)
        #expect(result.newOrUpdatedChains.first?.name == "Polkadot Updated")

        let stored = try await sut.fetchAll()
        #expect(stored.first?.name == "Polkadot Updated")
    }

    @Test("Skips chain when remote model is identical")
    func skipsUnchangedChain() async throws {
        let remote = ChainMock.makeRemoteChain(name: "Polkadot")
        let localChain = ChainMock.makeChainModel(from: remote, order: 0)

        let sut = try await makeSUT(
            localChains: [localChain],
            remoteChains: [remote]
        )

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        #expect(result.newOrUpdatedChains.isEmpty)
    }

    // MARK: - Remove Chains

    @Test("Removes chains not present in remote")
    func removesMissingChains() async throws {
        let remoteKept = ChainMock.makeRemoteChain(name: "Polkadot")
        let remoteRemoved = ChainMock.makeRemoteChain(name: "Old Chain")

        let localKept = ChainMock.makeChainModel(from: remoteKept, order: 0)
        let localRemoved = ChainMock.makeChainModel(from: remoteRemoved, order: 1)

        let sut = try await makeSUT(
            localChains: [localKept, localRemoved],
            remoteChains: [remoteKept]
        )

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        #expect(result.removedChains.count == 1)
        #expect(result.removedChains.first?.chainId == remoteRemoved.chainId)

        let stored = try await sut.fetchAll()
        #expect(stored.count == 1)
        #expect(stored.first?.chainId == remoteKept.chainId)
    }

    @Test("Removes all local chains when remote is empty")
    func removesAllWhenRemoteEmpty() async throws {
        let remote1 = ChainMock.makeRemoteChain(name: "Old 1")
        let remote2 = ChainMock.makeRemoteChain(name: "Old 2")

        let local1 = ChainMock.makeChainModel(from: remote1, order: 0)
        let local2 = ChainMock.makeChainModel(from: remote2, order: 1)

        let sut = try await makeSUT(
            localChains: [local1, local2],
            remoteChains: []
        )

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        #expect(result.removedChains.count == 2)

        let stored = try await sut.fetchAll()
        #expect(stored.isEmpty)
    }

    @Test("Does not remove when all local chains exist in remote")
    func noRemovalsWhenAllMatch() async throws {
        let remote = ChainMock.makeRemoteChain(name: "Polkadot")
        let localChain = ChainMock.makeChainModel(from: remote, order: 0)

        let sut = try await makeSUT(
            localChains: [localChain],
            remoteChains: [remote]
        )

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        #expect(result.removedChains.isEmpty)
    }

    // MARK: - Mixed CRUD

    @Test("Handles add, update and remove simultaneously")
    func mixedCRUD() async throws {
        let existingChainId = ChainMock.randomChainId()
        let remoteExisting = ChainMock.makeRemoteChain(id: existingChainId, name: "Polkadot")
        let remoteToRemove = ChainMock.makeRemoteChain(name: "Old")

        let localExisting = ChainMock.makeChainModel(from: remoteExisting, order: 0)
        let localRemoved = ChainMock.makeChainModel(from: remoteToRemove, order: 1)

        let updatedRemoteExisting = ChainMock.makeRemoteChain(id: existingChainId, name: "Polkadot v2")
        let remoteNew = ChainMock.makeRemoteChain(name: "New Chain")

        let sut = try await makeSUT(
            localChains: [localExisting, localRemoved],
            remoteChains: [updatedRemoteExisting, remoteNew]
        )

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        #expect(result.newOrUpdatedChains.count == 2)
        #expect(result.removedChains.count == 1)
        #expect(result.removedChains.first?.chainId == remoteToRemove.chainId)

        let stored = try await sut.fetchAll()
        #expect(stored.count == 2)
        #expect(stored.contains { $0.chainId == existingChainId && $0.name == "Polkadot v2" })
        #expect(stored.contains { $0.chainId == remoteNew.chainId })
    }

    // MARK: - Empty States

    @Test("Empty local and empty remote produces no changes")
    func emptySync() async throws {
        let sut = try await makeSUT()

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        #expect(result.newOrUpdatedChains.isEmpty)
        #expect(result.removedChains.isEmpty)

        let stored = try await sut.fetchAll()
        #expect(stored.isEmpty)
    }

    // MARK: - Order Preservation

    @Test("Preserves remote order as chain order")
    func preservesRemoteOrder() async throws {
        let remote1 = ChainMock.makeRemoteChain(name: "Alpha")
        let remote2 = ChainMock.makeRemoteChain(name: "Beta")
        let remote3 = ChainMock.makeRemoteChain(name: "Gamma")

        let sut = try await makeSUT(remoteChains: [remote1, remote2, remote3])

        sut.service.syncUpChains()
        _ = try await sut.awaitSyncComplete()

        let stored = try await sut.fetchAll().sorted { $0.order < $1.order }
        #expect(stored.count == 3)
        #expect(stored[0].order == 0)
        #expect(stored[0].chainId == remote1.chainId)
        #expect(stored[1].order == 1)
        #expect(stored[1].chainId == remote2.chainId)
        #expect(stored[2].order == 2)
        #expect(stored[2].chainId == remote3.chainId)
    }

    // MARK: - Events

    @Test("Fires ChainSyncDidComplete event on success")
    func firesCompleteEvent() async throws {
        let sut = try await makeSUT()

        sut.service.syncUpChains()
        let event = await sut.awaitSyncEvent()

        #expect(event is ChainSyncDidComplete)
    }

    // MARK: - Remote Config Failure Fallback

    @Test("Falls back to local chains as newOrUpdated when remote fetch fails")
    func fallbackToLocalOnRemoteError() async throws {
        let remote1 = ChainMock.makeRemoteChain(name: "Polkadot")
        let remote2 = ChainMock.makeRemoteChain(name: "Kusama")

        let local1 = ChainMock.makeChainModel(from: remote1, order: 0)
        let local2 = ChainMock.makeChainModel(from: remote2, order: 1)

        let sut = try await makeSUT(
            localChains: [local1, local2],
            remoteError: NSError(domain: "test", code: -1)
        )

        sut.service.syncUpChains()
        let result = try await sut.awaitSyncComplete()

        // When remote fails, mapping falls back to localModels as newOrUpdated
        #expect(result.newOrUpdatedChains.count == 2)
        #expect(result.removedChains.isEmpty)

        let stored = try await sut.fetchAll()
        #expect(stored.count == 2)
    }
}
