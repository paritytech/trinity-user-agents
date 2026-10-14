import Foundation
import Testing
import SubstrateSdk
@testable import polkadot_app

struct ChainLivenessAnchorProviderTests {
    // Chat is a 2s chain: 30s window, 15 slots.
    private static let slotCount = 15

    @Test("Span is the chain time between the head and the head minus the slot count")
    func spanAcrossSlotCount() async throws {
        let blockInfo = MockAnchorBlockInfoProvider()
        let chainTime = MockChainTimeProvider()

        await blockInfo.setCurrentHeight(1_000)
        await chainTime.setSeconds(1_000_090, forHeight: 1_000)
        await chainTime.setSeconds(1_000_000, forHeight: 985)

        let anchor = try await makeProvider(blockInfo, chainTime)
            .fetchAnchor(for: .chat, slotCount: Self.slotCount)

        #expect(anchor.headHeight == 1_000)
        #expect(anchor.chainTimeSpanSeconds == 90)
    }

    @Test("Both hashes are resolved from the same head height")
    func hashesResolvedFromOneHeight() async throws {
        // The timestamps are keyed by hash, so reading either at the wrong block would throw
        // rather than quietly returning the other block's time.
        let blockInfo = MockAnchorBlockInfoProvider()
        let chainTime = MockChainTimeProvider()

        await blockInfo.setCurrentHeight(500)
        await chainTime.setSeconds(2_000_030, forHeight: 500)
        await chainTime.setSeconds(2_000_000, forHeight: 485)

        _ = try await makeProvider(blockInfo, chainTime)
            .fetchAnchor(for: .chat, slotCount: Self.slotCount)

        let requested = await blockInfo.requestedHashHeights
        #expect(Set(requested) == [500, 485])
    }

    @Test("A chain shorter than the slot count throws instead of underflowing")
    func chainShorterThanSlotCountThrows() async {
        // BlockNumber is UInt32: 3 - 15 traps. This must be refused before the subtraction.
        let blockInfo = MockAnchorBlockInfoProvider()
        let chainTime = MockChainTimeProvider()

        await blockInfo.setCurrentHeight(3)

        let error = await capturedError(makeProvider(blockInfo, chainTime))

        #expect(error == .chainTooShort)
    }

    @Test("A head exactly at the slot count is allowed")
    func headExactlyAtSlotCountIsAllowed() async throws {
        let blockInfo = MockAnchorBlockInfoProvider()
        let chainTime = MockChainTimeProvider()

        await blockInfo.setCurrentHeight(15)
        await chainTime.setSeconds(1_000_030, forHeight: 15)
        await chainTime.setSeconds(1_000_000, forHeight: 0)

        let anchor = try await makeProvider(blockInfo, chainTime)
            .fetchAnchor(for: .chat, slotCount: Self.slotCount)

        #expect(anchor.headHeight == 15)
    }

    @Test("Equal timestamps throw rather than reporting a healthy chain")
    func zeroSpanThrows() async {
        let blockInfo = MockAnchorBlockInfoProvider()
        let chainTime = MockChainTimeProvider()

        await blockInfo.setCurrentHeight(1_000)
        await chainTime.setSeconds(1_000_000, forHeight: 1_000)
        await chainTime.setSeconds(1_000_000, forHeight: 985)

        let error = await capturedError(makeProvider(blockInfo, chainTime))

        #expect(error == .invalidTimeSpan)
    }

    @Test("A head older than its ancestor throws — the reads are off one lineage")
    func negativeSpanThrows() async {
        // Impossible on a single chain, so it means a reorg landed between the two lookups or a
        // load-balanced endpoint answered them from different backends.
        let blockInfo = MockAnchorBlockInfoProvider()
        let chainTime = MockChainTimeProvider()

        await blockInfo.setCurrentHeight(1_000)
        await chainTime.setSeconds(999_900, forHeight: 1_000)
        await chainTime.setSeconds(1_000_000, forHeight: 985)

        let error = await capturedError(makeProvider(blockInfo, chainTime))

        #expect(error == .invalidTimeSpan)
    }

    @Test("A target with no provider throws instead of reporting an anchor")
    func missingProviderThrows() async {
        let provider = ChainLivenessAnchorProvider(blockInfoProviders: [:], chainTimeProviders: [:])

        let error = await capturedError(provider)

        #expect(error == .blockInfoProviderUnavailable)
    }
}

private extension ChainLivenessAnchorProviderTests {
    func makeProvider(
        _ blockInfo: MockAnchorBlockInfoProvider,
        _ chainTime: MockChainTimeProvider
    ) -> ChainLivenessAnchorProvider {
        ChainLivenessAnchorProvider(
            blockInfoProviders: [.chat: blockInfo],
            chainTimeProviders: [.chat: chainTime]
        )
    }

    func capturedError(_ provider: ChainLivenessAnchorProvider) async -> ChainLivenessAnchorProviderError? {
        do {
            _ = try await provider.fetchAnchor(for: .chat, slotCount: Self.slotCount)
            return nil
        } catch let error as ChainLivenessAnchorProviderError {
            return error
        } catch {
            return nil
        }
    }
}
