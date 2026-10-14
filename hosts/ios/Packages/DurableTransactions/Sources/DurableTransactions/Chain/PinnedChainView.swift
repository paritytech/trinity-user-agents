import Foundation
import SubstrateOperation
import SubstrateSdk

/// Concrete ``PinnedChainViewProtocol`` over a block-info provider and a block-outcome reader.
///
/// Converts every failure mode — a transport error, an unknown block, an undecodable value — into
/// `failedRead` rather than `absent`, so no read failure can ever produce a terminal verdict.
final class PinnedChainView: PinnedChainViewProtocol, @unchecked Sendable {
    let chainId: ChainId
    let finalizedHead: BlockRef
    let bestHead: BlockRef

    private let blockInfoProvider: any BlockInfoProviding
    private let scan: BlockBodyScan

    init(
        chainId: ChainId,
        heads: ChainHeads,
        blockInfoProvider: any BlockInfoProviding,
        outcomeReader: any BlockOutcomeReading
    ) {
        self.chainId = chainId
        finalizedHead = heads.finalized
        bestHead = heads.best
        self.blockInfoProvider = blockInfoProvider
        scan = BlockBodyScan(outcomeReader: outcomeReader, blockInfoProvider: blockInfoProvider)
    }

    func blockHash(at number: UInt32) async -> ReadResult<Data> {
        guard let hash = try? await blockInfoProvider.fetchBlockHash(number) else {
            return .failedRead
        }
        return .present(hash)
    }

    func blockRef(forHash hash: Data) async -> ReadResult<BlockRef> {
        guard let number = try? await blockInfoProvider.fetchBlockNumber(byHash: hash) else {
            return .failedRead
        }
        return .present(BlockRef(number: number, hash: hash))
    }

    func dispatchOutcome(txHash: Data, at block: BlockRef) async -> ReadResult<DispatchOutcome> {
        await scan.outcome(of: txHash, at: block)
    }

    func searchBodies(for txHash: Data, in window: ClosedRange<UInt32>) async -> BodySearchOutcome {
        await scan.search(for: txHash, in: window)
    }
}

/// Scans block bodies for an extrinsic hash and reads its dispatch outcome.
///
/// The ladder's evidence of last resort: used when nothing a domain observed could decide a transaction.
/// Nothing is carried between passes — a pass that cannot read the whole window simply repeats it, which
/// is the same liveness either way, and the window is bounded at one mortality.
public struct BlockBodyScan {
    let outcomeReader: any BlockOutcomeReading
    let blockInfoProvider: any BlockInfoProviding

    public init(outcomeReader: any BlockOutcomeReading, blockInfoProvider: any BlockInfoProviding) {
        self.outcomeReader = outcomeReader
        self.blockInfoProvider = blockInfoProvider
    }

    /// Scans `window` for `txHash`, newest block first so a recent inclusion is found quickly.
    ///
    /// On a hit the dispatch outcome is read from the same block the extrinsic was found in — inclusion
    /// is not success, and only the events at that block say which.
    public func search(for txHash: Data, in window: ClosedRange<UInt32>) async -> BodySearchOutcome {
        var everyBlockRead = true

        for number in window.reversed() {
            guard let hash = try? await blockInfoProvider.fetchBlockHash(number) else {
                everyBlockRead = false
                continue
            }

            let block = BlockRef(number: number, hash: hash)

            switch await outcomeReader.lookUp(txHash, at: hash) {
            case .unreadable:
                everyBlockRead = false
            case .notInBlock:
                continue
            case let .outcome(result):
                return Self.mapSearchOutcome(result: result, block: block)
            }
        }

        return everyBlockRead ? .notFoundWindowComplete : .incomplete
    }

    /// Reads the outcome of `txHash` at `block`, resolving its index from the same block the events
    /// come from.
    public func outcome(of txHash: Data, at block: BlockRef) async -> ReadResult<DispatchOutcome> {
        switch await outcomeReader.lookUp(txHash, at: block.hash) {
        case let .outcome(result):
            result
        case .notInBlock,
             .unreadable:
            .failedRead
        }
    }

    private static func mapSearchOutcome(
        result: ReadResult<DispatchOutcome>,
        block: BlockRef
    ) -> BodySearchOutcome {
        switch result {
        case .present(.succeeded):
            .foundSucceeded(block)
        case let .present(.failed(reason)):
            .foundFailed(block, reason: reason)
        case .absent,
             .failedRead:
            .foundOutcomeUnreadable(block)
        }
    }
}
