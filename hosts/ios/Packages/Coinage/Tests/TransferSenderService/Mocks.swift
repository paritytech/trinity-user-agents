import Foundation
import BigInt
import SubstrateSdk
import Individuality
import Operation_iOS
import ExtrinsicService
import KeyDerivation
import BandersnatchApi
import SubstrateOperation
import AsyncExtensions
import BackgroundExecution

@testable import Coinage

extension TransferSenderServiceTests {
    final class MockMemoBuilder: MemoBuilding {
        func buildMemo(
            from entries: [PlannedMemoEntry],
            breakdownContext: DenominationBreakdownContext
        ) throws -> TransferMemo {
            let totalValue = entries.reduce(BigUInt(0)) {
                $0 + breakdownContext.valueInPlanks(for: $1.valueExponent)
            }
            return TransferMemo(entries: entries.map { _ in Data([0x00]) }, totalValue: totalValue)
        }
    }

    /// Mock coin allocator that returns coins with sequential derivation indices and records every
    /// coin it mints, so tests can assert on the outputs the strategies persist.
    actor MockCoinAllocator: CoinAllocating, CoinMinting {
        private var nextIndex: CoinageKeyIndex = 100
        private(set) var mintedCoins: [Coin] = []

        func allocate(exponent: Int16, provenance: CoinProvenance) async throws -> Coin {
            let index = nextIndex
            nextIndex = nextIndex.next()
            let coin = Coin(
                exponent: exponent,
                derivationIndex: index,
                age: nil,
                recyclerFungibility: provenance.recyclerFungibility,
                hops: provenance.hops,
                publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
            )
            mintedCoins.append(coin)
            return coin
        }

        func mintCoin(exponent: Int16, provenance: CoinProvenance) async throws -> Coin {
            try await allocate(exponent: exponent, provenance: provenance)
        }
    }

    /// Mock recycler loader that returns configured recycler states
    final class MockRecyclerLoader: RecyclerReadinessLoading, @unchecked Sendable {
        private let mutex = NSLock()

        private var storedStates: [RecyclerKey: MembersPallet.RingStatus] = [:]
        private var storedRevisions: [RecyclerKey: UInt32] = [:]
        private var storedMaxConsolidation: UInt32 = 100
        private var storedMaxSplitOutputs: UInt32 = 32

        var states: [RecyclerKey: MembersPallet.RingStatus] {
            get { mutex.withLock { storedStates } }
            set { mutex.withLock { storedStates = newValue } }
        }

        var revisions: [RecyclerKey: UInt32] {
            get { mutex.withLock { storedRevisions } }
            set { mutex.withLock { storedRevisions = newValue } }
        }

        var maxConsolidationValue: UInt32 {
            get { mutex.withLock { storedMaxConsolidation } }
            set { mutex.withLock { storedMaxConsolidation = newValue } }
        }

        var maxSplitOutputsValue: UInt32 {
            get { mutex.withLock { storedMaxSplitOutputs } }
            set { mutex.withLock { storedMaxSplitOutputs = newValue } }
        }

        func maxConsolidation() async throws -> UInt32 {
            maxConsolidationValue
        }

        func maxSplitOutputs() async throws -> UInt32 {
            maxSplitOutputsValue
        }

        func fetchRecyclerStates(for keys: [RecyclerKey]) async throws -> [RecyclerKey: MembersPallet.RingStatus] {
            var result: [RecyclerKey: MembersPallet.RingStatus] = [:]
            for key in keys {
                if let state = states[key] {
                    result[key] = state
                }
            }
            return result
        }

        func fetchRevisions(for keys: [RecyclerKey], blockHash _: BlockHashData?) async throws -> [
            RecyclerKey: UInt32
        ] {
            var result: [RecyclerKey: UInt32] = [:]
            for key in keys {
                if let revision = revisions[key] {
                    result[key] = revision
                }
            }
            return result
        }

        func subscribeRecyclerStates(
            for _: [RecyclerKey]
        ) -> AnyAsyncSequence<[RecyclerKey: MembersPallet.RingKeysStatus?]> {
            AsyncStream { _ in }.eraseToAnyAsyncSequence()
        }
    }

    /// Mock extrinsic submission monitor that simulates successful submissions
    final class MockExtrinsicSubmitMonitor: ExtrinsicSubmitMonitorFactoryProtocol {
        func submitAndMonitorWrapper(
            extrinsicBuilderClosure _: @escaping ExtrinsicBuilderClosure,
            origin _: ExtrinsicOriginDefining,
            params _: ExtrinsicSubmissionParams
        ) -> CompoundOperationWrapper<ExtrinsicMonitorSubmission> {
            .createWithResult(
                ExtrinsicMonitorSubmission(
                    extrinsicSubmittedModel: ExtrinsicSubmittedModel(
                        txHash: "0x" + String(repeating: "0", count: 64),
                        sender: .none
                    ),
                    status: .success(.init(
                        extrinsicHash: "0x" + String(repeating: "0", count: 64),
                        blockHash: "0x" + String(repeating: "1", count: 64),
                        blockNumber: 1,
                        extrinsicIndex: 0,
                        interestedEvents: []
                    ))
                )
            )
        }

        func submitAndMonitorWrapper(
            extrinsicBuilderClosure _: @escaping ExtrinsicBuilderIndexedClosure,
            origin _: ExtrinsicOriginDefining,
            indexes _: IndexSet,
            params _: ExtrinsicIndexedSubmissionParams
        ) -> CompoundOperationWrapper<ExtrinsicRetriableResult<ExtrinsicMonitorSubmission>> {
            .createWithError(TransferSenderServiceError.noSuitableCoins)
        }
    }

    /// Mock origin factory that returns mock origins
    final class MockBlockNumberProvider: BlockInfoProviding {
        func fetchCurrentHash() async throws -> SubstrateSdk.BlockHashData {
            Data(repeating: 0x00, count: 32)
        }

        func fetchCurrent() async throws -> BlockNumber {
            BlockNumber(123)
        }

        func fetchFinalized() async throws -> BlockNumber {
            BlockNumber(122)
        }

        func fetchFinalizedHash() async throws -> BlockHashData {
            Data(repeating: 0x00, count: 32)
        }

        func fetchBlockHash(_: BlockNumber) async throws -> BlockHashData {
            Data(repeating: 0x00, count: 32)
        }

        func fetchBlockNumber(byHash _: BlockHashData) async throws -> BlockNumber {
            BlockNumber(123)
        }

        func subscribeFinalizedHeads() -> AnyAsyncSequence<Block.Header> {
            AsyncStream<Block.Header> { _ in }.eraseToAnyAsyncSequence()
        }

        func subscribeNewHeads() -> AnyAsyncSequence<Block.Header> {
            AsyncStream<Block.Header> { $0.finish() }.eraseToAnyAsyncSequence()
        }
    }
}

struct StubUnloadQuotaTracker: UnloadQuotaTracking {
    func remainingQuota() async throws -> UnloadQuota { UnloadQuota(remaining: 0, limit: 0) }
    func noteUnloadHappened(count _: Int) async {}
}
