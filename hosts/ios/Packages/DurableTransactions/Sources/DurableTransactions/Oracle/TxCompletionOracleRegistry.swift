import Foundation
import os
import SubstrateSdk

/// The oracles the engine knows, keyed by domain. A domain registers its oracle before it submits
/// anything; a domain with none registered is skipped by the pass rather than guessed at, since without
/// an oracle there is no chain to pin.
public final class TxCompletionOracleRegistry: Sendable {
    private let oracles = OSAllocatedUnfairLock<[TxDomainId: any TxCompletionOracle]>(initialState: [:])

    public init() {}

    public func register(_ oracle: any TxCompletionOracle, for domain: TxDomainId) {
        oracles.withLock { $0[domain] = oracle }
    }

    public func oracle(for domain: TxDomainId) -> (any TxCompletionOracle)? {
        oracles.withLock { $0[domain] }
    }

    public func chainId(for domain: TxDomainId) -> ChainId? {
        oracle(for: domain)?.chainId
    }

    /// Every chain some registered domain lives on, deduplicated.
    public var chainIds: [ChainId] {
        var seen: Set<ChainId> = []
        return oracles.withLock { $0.values.map(\.chainId) }
            .filter { seen.insert($0).inserted }
    }
}
