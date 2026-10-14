import Foundation

/// One recycler's vouchers, sliced small enough for a single unload call.
///
/// The pallet bounds a call's `aliases` by `MaxConsolidation`, so a recycler holding more vouchers
/// than that is unloaded by several calls rather than rejected. Chunks of the same recycler are
/// disjoint and stay valid together: `unload` marks each alias individually and leaves the ring
/// revision untouched, so the calls can be in flight at once.
struct RecyclerVoucherChunk: Equatable {
    let key: RecyclerKey
    let vouchers: [Voucher]
}

enum RecyclerVoucherChunker {
    /// Groups vouchers by recycler, then slices each group into chunks of at most `maxPerChunk`.
    ///
    /// Groups keep the order their first voucher appeared in, and vouchers keep their order within
    /// a group, so the same input always yields the same chunks.
    static func chunk(_ vouchers: [Voucher], maxPerChunk: Int) throws -> [RecyclerVoucherChunk] {
        let chunkSize = max(maxPerChunk, 1)

        var order: [RecyclerKey] = []
        var grouped: [RecyclerKey: [Voucher]] = [:]

        for voucher in vouchers {
            guard let recycler = voucher.recycler else {
                throw CoinageCommonError.recyclerNotFound
            }
            let key = RecyclerKey(exponent: voucher.exponent, index: recycler.index)
            if grouped[key] == nil {
                order.append(key)
            }
            grouped[key, default: []].append(voucher)
        }

        return order.flatMap { key -> [RecyclerVoucherChunk] in
            let group = grouped[key] ?? []
            return stride(from: 0, to: group.count, by: chunkSize).map { start in
                RecyclerVoucherChunk(
                    key: key,
                    vouchers: Array(group[start ..< min(start + chunkSize, group.count)])
                )
            }
        }
    }
}
