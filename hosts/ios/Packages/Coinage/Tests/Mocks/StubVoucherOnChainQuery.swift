import Foundation
import os
@testable import Coinage

/// Answers from a per-block table (`nil` block = best head) and records every read.
final class StubVoucherOnChainQuery: VoucherOnChainQuerying, @unchecked Sendable {
    struct Read: Equatable {
        let indices: [CoinageKeyIndex]
        let atBlockHash: Data?
    }

    private let state = OSAllocatedUnfairLock<(
        infos: [Data?: [CoinageKeyIndex: VoucherOnChainInfo]],
        reads: [Read]
    )>(initialState: ([:], []))

    var reads: [Read] { state.withLock { $0.reads } }

    func setInfo(_ info: VoucherOnChainInfo, for index: CoinageKeyIndex, atBlockHash blockHash: Data?) {
        state.withLock { $0.infos[blockHash, default: [:]][index] = info }
    }

    func fetchVouchers(
        for derivationIndices: [CoinageKeyIndex],
        atBlockHash: Data?
    ) async throws -> [VoucherOnChainInfo?] {
        state.withLock { state in
            state.reads.append(Read(indices: derivationIndices, atBlockHash: atBlockHash))
            let table = state.infos[atBlockHash] ?? [:]
            return derivationIndices.map { table[$0] }
        }
    }
}
