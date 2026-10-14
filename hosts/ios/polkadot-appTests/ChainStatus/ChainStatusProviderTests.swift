import Foundation
import Testing
import AsyncExtensions
import SubstrateSdk
import PolkadotUI
import FoundationExt
import os
@testable import polkadot_app

struct ChainStatusProviderTests {
    @Test("A row with no prior emission emits its raw indication")
    func firstEmissionRaw() async {
        let provider = makeProvider()
        let t0 = Date()

        await provider.handleStatusUpdate(.waitingForNetwork, for: .chat)
        await provider.emitRows(at: t0)

        let chatRow = await currentChatRow(from: provider)

        #expect(chatRow?.indication == .dead, "first emission with offline state is raw dead")
    }

    @Test("A stalling chain produces outage")
    func stallingChainOutage() async {
        // Chat is a 2s chain: 30s window, 15 slots. Blocks arrive on time up to t0+30, then stop.
        let mockAnchor = MockChainLivenessAnchorProvider()
        // Healthy anchor prevents clear on initial connect
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 30))
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        await awaitAnchor(for: .chat, from: provider)

        for index in 0 ... 15 {
            let date = t0.addingTimeInterval(Double(index) * 2)
            let blockInfo = ChainBlockInfo(
                number: BlockNumber(index),
                receivedAt: date,
                finalizedNumber: nil
            )
            await provider.handleBlocksUpdate([.chat: blockInfo], at: date)
        }

        var chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal, "15 blocks over the 15-slot window is liveness 1")

        // 10s into the stall the window start is t0+10, so the anchor is height 5 and 10 of the
        // 15 slots carry a block. The exact value pins the sample timestamps: if they collapsed
        // onto one instant, the anchor would be the head and this would read 0.
        await provider.emitRows(at: t0.addingTimeInterval(40))

        chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .outage(liveness: 10.0 / 15.0))

        // Window start is t0+31, past every sample: the anchor collapses onto the head and
        // liveness reads 0.
        await provider.emitRows(at: t0.addingTimeInterval(61))

        chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .outage(liveness: 0))
    }

    @Test("Disconnect clears liveness so reconnect does not inherit pre-drop history")
    func disconnectClearsLiveness() async {
        // The gate keeps every probe parked, so no anchor ever touches history and the only thing
        // that can clear it is the disconnect. Without the gate a failed probe clears history too,
        // and the test could not tell the two apart.
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.closeGate()
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        for index in 0 ... 15 {
            let date = t0.addingTimeInterval(Double(index) * 2)
            let blockInfo = ChainBlockInfo(
                number: BlockNumber(index),
                receivedAt: date,
                finalizedNumber: nil
            )
            await provider.handleBlocksUpdate([.chat: blockInfo], at: date)
        }

        await provider.emitRows(at: t0.addingTimeInterval(40))

        var chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .outage(liveness: 10.0 / 15.0), "history contributes")

        await provider.handleStatusUpdate(.waitingForNetwork, for: .chat, at: t0.addingTimeInterval(50))
        await provider.emitRows(at: t0.addingTimeInterval(53.1))

        chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .dead)

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0.addingTimeInterval(54))
        await provider.emitRows(at: t0.addingTimeInterval(55))

        // Were the pre-drop samples still there, the window at t0+55 would read 3 of 15 slots.
        chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal, "pre-drop history did not survive the reconnect")

        await mockAnchor.release()
    }

    @Test("Foreground re-anchors every connected chain")
    func foregroundReanchorsEveryConnectedChain() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)
        await provider.handleStatusUpdate(.connected, for: .assethub, at: t0)
        await provider.handleStatusUpdate(.waitingForNetwork, for: .bulletin, at: t0)

        await awaitAnchor(for: .chat, from: provider)
        await awaitAnchor(for: .assethub, from: provider)

        await provider.handleForeground(at: t0)

        await awaitAnchor(for: .chat, from: provider)
        await awaitAnchor(for: .assethub, from: provider)

        let allCalls = await mockAnchor.fetchAnchorCalls
        let foregroundCalls = Array(allCalls.suffix(2))

        #expect(allCalls.count == 4)
        #expect(foregroundCalls.count == 2)
        #expect(foregroundCalls.contains { $0.target == .chat })
        #expect(foregroundCalls.contains { $0.target == .assethub })
        #expect(!allCalls.contains { $0.target == .bulletin })
    }

    @Test("The last indication is held until the probe lands")
    func lastIndicationHeldUntilProbeLands() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.closeGate()
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        for index in 0 ... 15 {
            let date = t0.addingTimeInterval(Double(index) * 2)
            let blockInfo = ChainBlockInfo(
                number: BlockNumber(index),
                receivedAt: date,
                finalizedNumber: nil
            )
            await provider.handleBlocksUpdate([.chat: blockInfo], at: date)
        }

        await provider.emitRows(at: t0.addingTimeInterval(40))

        var chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .outage(liveness: 10.0 / 15.0), "history builds a known outage")

        // Foreground at t0+70, where the stale history computes to liveness 0. The held value and
        // the computed value must differ, or the assertion would prove nothing.
        await provider.handleForeground(at: t0.addingTimeInterval(70))

        chatRow = await currentChatRow(from: provider)
        #expect(
            chatRow?.indication == .outage(liveness: 10.0 / 15.0),
            "held at the prior value, not the stale-history 0"
        )

        let foregroundProbe = await provider.anchorTasks[.chat]
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 30))
        await mockAnchor.openGate()
        await mockAnchor.release()

        await awaitAnchor(foregroundProbe)
        chatRow = await currentChatRow(from: provider)

        #expect(chatRow?.indication == .normal, "the landed anchor replaces the held value")
    }

    @Test("A healthy chain does not flash an outage after minutes away")
    func healthyChainNoOutageAfterAwayTime() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.closeGate()
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        for index in 0 ... 15 {
            let date = t0.addingTimeInterval(Double(index) * 2)
            let blockInfo = ChainBlockInfo(
                number: BlockNumber(index),
                receivedAt: date,
                finalizedNumber: nil
            )
            await provider.handleBlocksUpdate([.chat: blockInfo], at: date)
        }

        await provider.emitRows(at: t0.addingTimeInterval(30))

        var chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal, "a full window of blocks is liveness 1")

        // Three minutes away. The stale history now computes to liveness 0 — the false alarm this
        // ticket exists to prevent — so the hold is the only reason this stays normal.
        await provider.handleForeground(at: t0.addingTimeInterval(180))

        chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal, "no outage flashes while the probe is outstanding")

        let foregroundProbe = await provider.anchorTasks[.chat]
        // Span 33 lands at 13/15: still normal, but distinct from the held liveness 1, so the assertion proves the
        // probe applied.
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 33))
        await mockAnchor.openGate()
        await mockAnchor.release()

        await awaitAnchor(foregroundProbe)
        chatRow = await currentChatRow(from: provider)

        #expect(chatRow?.indication == .normal, "and still normal once the probe lands")
        #expect(chatRow?.liveness == 13.0 / 15.0, "the landed anchor replaced the held liveness 1")
    }

    @Test("A failed re-anchor clears history and reads normal")
    func failedReanchorClearsHistoryAndNormal() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        // Set healthy anchor initially
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 30))
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        // Build history
        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        await awaitAnchor(for: .chat, from: provider)

        for index in 0 ... 15 {
            let date = t0.addingTimeInterval(Double(index) * 2)
            let blockInfo = ChainBlockInfo(
                number: BlockNumber(index),
                receivedAt: date,
                finalizedNumber: nil
            )
            await provider.handleBlocksUpdate([.chat: blockInfo], at: date)
        }

        await provider.emitRows(at: t0.addingTimeInterval(40))

        var chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .outage(liveness: 10.0 / 15.0))

        // Now set error for foreground re-anchor
        await mockAnchor.setError(NSError(domain: "test", code: -1))
        await provider.handleForeground(at: t0.addingTimeInterval(41))

        // Wait for the failed probe to complete
        await awaitAnchor(for: .chat, from: provider)
        chatRow = await currentChatRow(from: provider)

        #expect(chatRow?.indication == .normal, "failed re-anchor clears history, row reads normal")
    }

    @Test("A hold does not mask a dead row")
    func holdDoesNotMaskDeadRow() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.closeGate()
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        for index in 0 ... 15 {
            let date = t0.addingTimeInterval(Double(index) * 2)
            let blockInfo = ChainBlockInfo(
                number: BlockNumber(index),
                receivedAt: date,
                finalizedNumber: nil
            )
            await provider.handleBlocksUpdate([.chat: blockInfo], at: date)
        }

        await provider.emitRows(at: t0.addingTimeInterval(30))

        var chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal)

        // Probe outstanding, so the row is held. Dead is state-driven, not liveness-driven, and
        // must come through anyway.
        await provider.handleForeground(at: t0.addingTimeInterval(180))
        await provider.handleStatusUpdate(.waitingForNetwork, for: .chat, at: t0.addingTimeInterval(181))

        // Past the 3s dead dwell, which is the only thing that should have delayed it.
        await provider.emitRows(at: t0.addingTimeInterval(185))

        chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .dead, "the hold never masks a dropped socket")

        await mockAnchor.release()
    }

    @Test("A superseded anchor completion is a no-op")
    func supersededAnchorCompletionIsNoop() async {
        // Two anchors in flight for one target: the connect probe is parked while the foreground
        // probe runs to completion. The parked one is stale by the time it resumes.
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.closeGate()
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        let connectProbe = await provider.anchorTasks[.chat]
        await mockAnchor.openGate()
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 30))

        await provider.handleForeground(at: t0.addingTimeInterval(1))

        await awaitAnchor(for: .chat, from: provider)
        var chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal, "the foreground anchor applied")
        #expect(chatRow?.liveness == 1, "the foreground anchor applied")

        // The parked connect probe now resumes carrying a much worse reading.
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 90))
        await mockAnchor.release()

        await awaitAnchor(connectProbe)
        chatRow = await currentChatRow(from: provider)

        #expect(chatRow?.indication == .normal, "the superseded completion changed nothing")
        #expect(chatRow?.liveness == 1, "a span-90 anchor would have read 5/15")
    }

    @Test("Recovery within dwell window clears deadSince so fresh dwell can start")
    func recoveryWithinDwellClearsDeadSince() async {
        // Fails if deadSince[rowId] = nil is removed from the (.normal, .normal) arm of applyDwell.
        // A fresh dwell must start when entering dead again, not reuse an old deadSince timestamp.
        let provider = makeProvider()
        let t0 = Date()

        // Step 1: Connect at t0, row is normal
        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)
        await provider.emitRows(at: t0)

        var chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal)

        // Step 2: Go offline at t0+1, dwell holds so row stays normal
        await provider.handleStatusUpdate(.waitingForNetwork, for: .chat, at: t0 + 1)
        await provider.emitRows(at: t0 + 1)

        chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal, "dwell holds dead within 3s window")

        // Step 3: Reconnect at t0+2 (within dwell), recovery clears deadSince
        await provider.handleStatusUpdate(.connected, for: .chat, at: t0 + 2)
        await provider.emitRows(at: t0 + 2)

        chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal)

        // Step 4: Go offline again at t0+10, a fresh dwell begins (not reusing old t0+1)
        await provider.handleStatusUpdate(.waitingForNetwork, for: .chat, at: t0 + 10)
        await provider.emitRows(at: t0 + 10)

        chatRow = await currentChatRow(from: provider)
        #expect(
            chatRow?.indication == .normal,
            "fresh dwell starts at t0+10, so nothing has elapsed yet and the dwell still holds"
        )

        // Step 5: At t0+13.1, the fresh dwell (3s from t0+10) expires and row goes dead
        await provider.emitRows(at: t0.addingTimeInterval(13.1))

        chatRow = await currentChatRow(from: provider)
        #expect(
            chatRow?.indication == .dead,
            "fresh dwell expired at t0+13: (t0+13.1 - t0+10 = 3.1s > 3s dwell)"
        )
    }

    @Test("Cold connect fetches exactly one anchor")
    func coldConnectFetchesOneAnchor() async {
        // A stalled anchor, not a healthy one: span 90 over a 30s window is 5 of 15 slots, so
        // the row can only read outage if the anchor was actually applied. A healthy anchor
        // would read normal, which is also what an un-applied anchor reads.
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 90))
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        await awaitAnchor(for: .chat, from: provider)
        let chatRow = await currentChatRow(from: provider)

        #expect(await mockAnchor.fetchAnchorCalls.count == 1)
        #expect(await mockAnchor.fetchAnchorCalls[0].target == .chat)
        #expect(await mockAnchor.fetchAnchorCalls[0].slotCount == 15)

        #expect(chatRow?.indication == .outage(liveness: 5.0 / 15.0), "anchor was applied")
    }

    @Test("Status change not into connected does not fetch anchor")
    func notConnectedDoesNotFetchAnchor() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connecting, for: .chat, at: t0)
        await provider.handleStatusUpdate(.waitingForNetwork, for: .chat, at: t0)

        #expect(await provider.anchorTasks.isEmpty)
        #expect(await mockAnchor.fetchAnchorCalls.isEmpty)
    }

    @Test("fetchAnchor error leaves row un-anchored")
    func fetchAnchorErrorLeavesRowUnanchored() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.setError(NSError(domain: "test", code: -1))
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        await awaitAnchor(for: .chat, from: provider)

        let chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.indication == .normal, "error does not crash; row stays on default indication")
    }

    @Test("A full window of blocks publishes liveness 1")
    func fullWindowBlocksPublishesLiveness1() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 30))
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        await awaitAnchor(for: .chat, from: provider)

        for index in 0 ... 15 {
            let date = t0.addingTimeInterval(Double(index) * 2)
            let blockInfo = ChainBlockInfo(
                number: BlockNumber(index),
                receivedAt: date,
                finalizedNumber: nil
            )
            await provider.handleBlocksUpdate([.chat: blockInfo], at: date)
        }

        await provider.emitRows(at: t0.addingTimeInterval(30))

        let chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.liveness == 1, "full window of blocks yields liveness 1")
    }

    @Test("A stalling chain publishes the liveness its indication carries")
    func stallingChainPublishesIndicationLiveness() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 30))
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        await awaitAnchor(for: .chat, from: provider)

        for index in 0 ... 15 {
            let date = t0.addingTimeInterval(Double(index) * 2)
            let blockInfo = ChainBlockInfo(
                number: BlockNumber(index),
                receivedAt: date,
                finalizedNumber: nil
            )
            await provider.handleBlocksUpdate([.chat: blockInfo], at: date)
        }

        await provider.emitRows(at: t0.addingTimeInterval(40))

        let chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.liveness == 10.0 / 15.0, "stalling chain publishes its outage liveness")
    }

    @Test("A held row keeps the previous liveness")
    func heldRowKeptPreviousLiveness() async {
        let mockAnchor = MockChainLivenessAnchorProvider()
        await mockAnchor.closeGate()
        let provider = makeProvider(anchorProvider: mockAnchor)
        let t0 = Date()

        await provider.handleStatusUpdate(.connected, for: .chat, at: t0)

        for index in 0 ... 15 {
            let date = t0.addingTimeInterval(Double(index) * 2)
            let blockInfo = ChainBlockInfo(
                number: BlockNumber(index),
                receivedAt: date,
                finalizedNumber: nil
            )
            await provider.handleBlocksUpdate([.chat: blockInfo], at: date)
        }

        await provider.emitRows(at: t0.addingTimeInterval(40))

        var chatRow = await currentChatRow(from: provider)
        #expect(chatRow?.liveness == 10.0 / 15.0, "history builds liveness 10/15")

        await provider.handleForeground(at: t0.addingTimeInterval(70))

        chatRow = await currentChatRow(from: provider)
        #expect(
            chatRow?.liveness == 10.0 / 15.0,
            "held at the prior liveness, not the stale-history 0"
        )

        let foregroundProbe = await provider.anchorTasks[.chat]
        await mockAnchor.setAnchor(ChainLivenessAnchor(headHeight: 100, chainTimeSpanSeconds: 30))
        await mockAnchor.openGate()
        await mockAnchor.release()

        await awaitAnchor(foregroundProbe)
        chatRow = await currentChatRow(from: provider)

        #expect(chatRow?.liveness == 1, "the landed anchor updates liveness")
    }
}

private extension ChainStatusProviderTests {
    func makeProvider(anchorProvider: ChainLivenessAnchorProviding? = nil) -> ChainStatusProvider {
        ChainStatusProvider(
            networkStatusService: MockNetworkStatusService(),
            blockProvider: MockChainBlockProvider(),
            anchorProvider: anchorProvider ?? MockChainLivenessAnchorProvider(),
            appStateStreamFactory: ApplicationStateStreamFactory(),
            statementStoreStatusProvider: MockStatementStoreStatusProvider(),
            chainRegistry: MockChainRegistry(),
            logger: StubLogger()
        )
    }

    func currentChatRow(from provider: ChainStatusProvider) async -> ChainConnectionStatusViewModel? {
        let rows = try? await provider.statusStream().first { _ in true }
        return rows?.first { $0.id == ChainConnectionTarget.chat.chainId }
    }

    func awaitAnchor(
        _ task: Task<Void, Never>?,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async {
        guard let task else {
            Issue.record("no anchor probe was started", sourceLocation: sourceLocation)
            return
        }

        // Awaiting `task.value` cannot be cancelled, so a task-group timeout would wait on it forever.
        // CI has also delivered probe completions seconds late, hence the generous bound.
        let finished = OSAllocatedUnfairLock(initialState: false)
        Task {
            await task.value
            finished.withLock { $0 = true }
        }

        let deadline = ContinuousClock.now + .seconds(30)
        while !finished.withLock({ $0 }), ContinuousClock.now < deadline {
            try? await Task.sleep(for: .milliseconds(10))
        }

        if !finished.withLock({ $0 }) {
            Issue.record("anchor probe did not finish within 30s", sourceLocation: sourceLocation)
        }
    }

    func awaitAnchor(
        for target: ChainConnectionTarget,
        from provider: ChainStatusProvider,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async {
        await awaitAnchor(provider.anchorTasks[target], sourceLocation: sourceLocation)
    }
}
