import Foundation
import Operation_iOS
import SubstrateSdk
import Testing
import AsyncExtensions
import ChainRegistry

@testable import polkadot_app

@Suite("NetworkStatusService Tests")
struct NetworkStatusServiceTests {
    static let chatChain = "chat-chain"
    static let relayChain = "relay-chain"
    static let nodeURL = URL(string: "wss://node.example")!

    @Test("emits connecting initially and connected once the scoped chain connects")
    func emitsConnectedWhenChainConnects() async throws {
        let harness = Harness()
        let stream = harness.service.statusStream(for: [Self.chatChain])
        var iterator = stream.makeAsyncIterator()

        #expect(try await iterator.next() == .connecting)

        harness.send(state: .connected(url: Self.nodeURL), for: Self.chatChain)

        #expect(try await iterator.next() == .connected)
        #expect(harness.registry.subscribedChainIds == [Self.chatChain])
    }

    @Test("does not emit duplicate statuses for distinct engine states")
    func skipsDuplicateStatuses() async throws {
        let harness = Harness()
        let stream = harness.service.statusStream(for: [Self.chatChain])
        var iterator = stream.makeAsyncIterator()

        #expect(try await iterator.next() == .connecting)

        harness.send(state: .waitingReconnection(url: Self.nodeURL), for: Self.chatChain)
        harness.send(state: .connecting(url: Self.nodeURL), for: Self.chatChain)
        harness.send(state: .connected(url: Self.nodeURL), for: Self.chatChain)

        #expect(try await iterator.next() == .connected)
    }

    @Test("path loss overrides chain state with waitingForNetwork")
    func pathLossProducesWaitingForNetwork() async throws {
        let harness = Harness()
        let stream = harness.service.statusStream(for: [Self.chatChain])
        var iterator = stream.makeAsyncIterator()

        #expect(try await iterator.next() == .connecting)

        harness.send(state: .connected(url: Self.nodeURL), for: Self.chatChain)
        #expect(try await iterator.next() == .connected)

        harness.pathMonitor.send(false)
        #expect(try await iterator.next() == .waitingForNetwork)

        harness.pathMonitor.send(true)
        #expect(try await iterator.next() == .connected)
    }

    @Test("empty scope reflects path availability only")
    func emptyScopeReflectsPathOnly() async throws {
        let harness = Harness()
        let stream = harness.service.statusStream(for: [])
        var iterator = stream.makeAsyncIterator()

        #expect(try await iterator.next() == .connected)

        harness.pathMonitor.send(false)
        #expect(try await iterator.next() == .waitingForNetwork)

        harness.pathMonitor.send(true)
        #expect(try await iterator.next() == .connected)
        #expect(harness.registry.subscribedChainIds.isEmpty)
    }

    @Test("scope with a lagging chain stays connecting while narrower scope connects")
    func scopedStreamsAreIndependent() async throws {
        let harness = Harness()

        let narrowStream = harness.service.statusStream(for: [Self.chatChain])
        var narrowIterator = narrowStream.makeAsyncIterator()

        let wideStream = harness.service.statusStream(for: [Self.chatChain, Self.relayChain])
        var wideIterator = wideStream.makeAsyncIterator()

        #expect(try await narrowIterator.next() == .connecting)
        #expect(try await wideIterator.next() == .connecting)

        harness.send(state: .connected(url: Self.nodeURL), for: Self.chatChain)

        #expect(try await narrowIterator.next() == .connected)

        harness.send(state: .connected(url: Self.nodeURL), for: Self.relayChain)

        #expect(try await wideIterator.next() == .connected)
    }

    @Test("repeated scopes subscribe to the registry once per chain")
    func repeatedScopesSubscribeOncePerChain() {
        let harness = Harness()

        _ = harness.service.statusStream(for: [Self.chatChain])
        _ = harness.service.statusStream(for: [Self.chatChain, Self.relayChain])
        _ = harness.service.statusStream(for: [Self.chatChain])

        #expect(harness.registry.subscribeCallCount(for: Self.chatChain) == 1)
        #expect(harness.registry.subscribeCallCount(for: Self.relayChain) == 1)
        #expect(harness.registry.subscribedChainIds == [Self.chatChain, Self.relayChain])
    }
}

private extension NetworkStatusServiceTests {
    final class Harness {
        let registry = RecordingChainRegistry()
        let pathMonitor = StubPathMonitor()
        let service: NetworkStatusService

        init() {
            service = NetworkStatusService(chainRegistry: registry, pathMonitor: pathMonitor)
        }

        func send(state: WebSocketEngine.State, for chainId: ChainModel.Id) {
            guard let subscriber = registry.subscriber(for: chainId) else {
                Issue.record("No subscriber registered for \(chainId)")
                return
            }

            subscriber.didReceive(state: state, for: chainId)
        }
    }

    final class RecordingChainRegistry: ChainRegistryProtocol, @unchecked Sendable {
        private let mutex = NSLock()
        private var subscribers: [ChainModel.Id: ConnectionStateSubscription] = [:]
        private var subscribeCalls: [ChainModel.Id: Int] = [:]

        var subscribedChainIds: Set<ChainModel.Id> {
            mutex.lock()
            defer { mutex.unlock() }
            return Set(subscribeCalls.keys)
        }

        func subscribeCallCount(for chainId: ChainModel.Id) -> Int {
            mutex.lock()
            defer { mutex.unlock() }
            return subscribeCalls[chainId] ?? 0
        }

        func subscriber(for chainId: ChainModel.Id) -> ConnectionStateSubscription? {
            mutex.lock()
            defer { mutex.unlock() }
            return subscribers[chainId]
        }

        func subscribeChainState(_ subscriber: ConnectionStateSubscription, chainId: ChainModel.Id) {
            mutex.lock()
            defer { mutex.unlock() }
            subscribeCalls[chainId, default: 0] += 1
            subscribers[chainId] = subscriber
        }

        func unsubscribeChainState(_: ConnectionStateSubscription, chainId: ChainModel.Id) {
            mutex.lock()
            defer { mutex.unlock() }
            subscribers[chainId] = nil
        }

        var availableChainIds: Set<ChainModel.Id>? { nil }
        var allAvailableChains: [ChainModel] { [] }

        func retainConnections(_: ConnectionRetainScope) -> ConnectionRetainToken { ConnectionRetainToken() }

        func getChain(for _: ChainModel.Id) -> ChainModel? { nil }
        func getChainByGenesis(for _: ChainModel.Id) -> ChainModel? { nil }
        func getConnection(for _: ChainModel.Id) -> ChainConnection? { nil }
        func getOneShotConnection(for _: ChainModel.Id) -> JSONRPCEngine? { nil }
        func setConnectionEnforced(_: Bool, for _: ChainModel.Id) {}
        func getRuntimeProvider(for _: ChainModel.Id) -> RuntimeProviderProtocol? { nil }
        func switchSync(mode _: ChainSyncMode, chainId _: ChainModel.Id) throws {}
        func chainsSubscribe(
            _: AnyObject,
            runningInQueue _: DispatchQueue,
            updateClosure _: @escaping ([DataProviderChange<ChainModel>]) -> Void
        ) {}
        func chainsUnsubscribe(_: AnyObject) {}
        func syncUp() {}
    }

    final class StubPathMonitor: NetworkPathMonitoring {
        private let mutex = NSLock()
        private var continuation: AsyncStream<Bool>.Continuation?

        func pathStream() -> AnyAsyncSequence<Bool> {
            let (stream, continuation) = AsyncStream<Bool>.makeStream()

            mutex.lock()
            self.continuation = continuation
            mutex.unlock()

            return stream.eraseToAnyAsyncSequence()
        }

        func send(_ isAvailable: Bool) {
            mutex.lock()
            defer { mutex.unlock() }
            continuation?.yield(isAvailable)
        }
    }
}
