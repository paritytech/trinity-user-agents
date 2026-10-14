import Foundation

/// Mints fresh coins by allocating a derivation index and persisting the row, so a coin exists in
/// the database from the moment it is minted.
protocol CoinMinting: Sendable {
    func mintCoin(exponent: Int16, provenance: CoinProvenance) async throws -> Coin
}

/// Mints fresh vouchers, persisting each on allocation.
protocol VoucherMinting: Sendable {
    func mintVoucher(exponent: Int16) async throws -> Voucher
}

extension CoinMinting {
    /// Mints one coin per exponent, in order, all sharing one provenance — the operation that
    /// produced them is the same for every output.
    func mintCoins(_ exponents: [Int16], provenance: CoinProvenance) async throws -> [Coin] {
        var coins: [Coin] = []
        for exponent in exponents {
            try await coins.append(mintCoin(exponent: exponent, provenance: provenance))
        }
        return coins
    }
}

extension VoucherMinting {
    /// Mints one voucher per exponent, in order.
    func mintVouchers(_ exponents: [Int16]) async throws -> [Voucher] {
        var vouchers: [Voucher] = []
        for exponent in exponents {
            try await vouchers.append(mintVoucher(exponent: exponent))
        }
        return vouchers
    }
}

/// The single minting entry point for coinage.
typealias CoinageMinting = CoinMinting & VoucherMinting

/// Wraps the coin and voucher allocators so every site mints through one shared instance. The
/// allocators are actors whose isolation serialises their index counters, so a single
/// ``CoinageMinter`` per Coinage instance is the only safe setup — the same constraint as the
/// allocators it wraps.
final class CoinageMinter: CoinageMinting {
    private let coinAllocator: any CoinAllocating
    private let voucherAllocator: any VoucherAllocating

    init(coinAllocator: any CoinAllocating, voucherAllocator: any VoucherAllocating) {
        self.coinAllocator = coinAllocator
        self.voucherAllocator = voucherAllocator
    }

    func mintCoin(exponent: Int16, provenance: CoinProvenance) async throws -> Coin {
        try await coinAllocator.allocate(exponent: exponent, provenance: provenance)
    }

    func mintVoucher(exponent: Int16) async throws -> Voucher {
        try await voucherAllocator.allocate(exponent: exponent)
    }
}
