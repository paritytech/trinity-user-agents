import Foundation
import FoundationExt
import PolkadotUI
import StructuredConcurrency
import Testing

@testable import polkadot_app

@Suite("Chain status provider: statement store ring")
struct ChainStatusProviderStatementStoreTests {
    @Test("The statement store is the last ring and starts out connecting")
    func statementStoreRowIsSeeded() async {
        let (provider, _) = makeProvider()

        let rows = try? await provider.statusStream().first { _ in true }
        let row = rows?.last

        #expect(row?.id == ChainStatusProvider.statementStoreRowId)
        #expect(row?.state == .connecting)
        #expect(row?.showsChainMetrics == false)
        #expect(rows?.count == ChainConnectionTarget.allCases.count + 1)
    }

    @Test("Statement store status drives its ring without touching the chain rows")
    func statementStoreStatusDrivesRow() async {
        let (provider, _) = makeProvider()
        let start = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: start)
        await provider.handleStatementStoreUpdate(.active, at: start)
        var row = await row(ChainStatusProvider.statementStoreRowId, from: provider)
        #expect(row?.state == .connected)
        #expect(row?.indication == .normal)

        await provider.handleStatementStoreUpdate(.unavailable, at: start.addingTimeInterval(10))
        await provider.emitRows(at: start.addingTimeInterval(14))
        row = await self.row(ChainStatusProvider.statementStoreRowId, from: provider)
        #expect(row?.state == .offline)
        #expect(row?.indication == .dead)

        let chatRow = await self.row(ChainConnectionTarget.chat.chainId, from: provider)
        #expect(chatRow?.state == .connected)
    }

    @Test("Starting the provider starts the status source and follows it")
    func startFollowsStatusSource() async throws {
        let (provider, statusProvider) = makeProvider()

        await provider.start()
        #expect(statusProvider.startCallCount == 1)

        statusProvider.simulateStatus(.active)

        let connected = await waitForRow(ChainStatusProvider.statementStoreRowId, from: provider) {
            $0.state == .connected
        }
        #expect(connected?.state == .connected)
    }
}

private extension ChainStatusProviderStatementStoreTests {
    func makeProvider() -> (ChainStatusProvider, MockStatementStoreStatusProvider) {
        let statusProvider = MockStatementStoreStatusProvider()
        let provider = ChainStatusProvider(
            networkStatusService: MockNetworkStatusService(),
            blockProvider: MockChainBlockProvider(),
            anchorProvider: MockChainLivenessAnchorProvider(),
            appStateStreamFactory: ApplicationStateStreamFactory(),
            statementStoreStatusProvider: statusProvider,
            chainRegistry: MockChainRegistry(),
            logger: StubLogger()
        )
        return (provider, statusProvider)
    }

    func row(_ id: String, from provider: ChainStatusProvider) async -> ChainConnectionStatusViewModel? {
        let rows = try? await provider.statusStream().first { _ in true }
        return rows?.first { $0.id == id }
    }

    func waitForRow(
        _ id: String,
        from provider: ChainStatusProvider,
        where predicate: @escaping (ChainConnectionStatusViewModel) -> Bool
    ) async -> ChainConnectionStatusViewModel? {
        let deadline = ContinuousClock.now + .seconds(60)
        while ContinuousClock.now < deadline {
            if let row = await row(id, from: provider), predicate(row) {
                return row
            }
            try? await Task.sleep(for: .milliseconds(20))
        }
        return await row(id, from: provider)
    }
}
