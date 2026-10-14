import Foundation

extension TrackedVoucher: CustomDebugStringConvertible {
    public var debugDescription: String {
        "derivationIndex=\(voucher.derivationIndex) exponent=\(voucher.exponent) " +
            "remote=\(voucher.remoteState.debugDescription) " +
            "consumer=\(String(describing: state.consumerStatus)) " +
            "minter=\(String(describing: state.minterStatus)) "
    }
}

extension Voucher.OnChainState: CustomDebugStringConvertible {
    public var debugDescription: String {
        switch self {
        case .onboarding:
            "Onboarding"
        case .unlocated:
            "Unlocated"
        case let .inRecycler(recycler):
            "In Recycler: \(recycler.debugDescription)"
        }
    }
}

extension Voucher.Recycler: CustomDebugStringConvertible {
    public var debugDescription: String {
        "index=\(index), ringMembers=\(membersCount)"
    }
}

public extension [CoinageKeyIndex: Voucher.OnChainState] {
    var toDebugDescription: String {
        var log = "Voucher dict: \(count): "

        for (index, state) in sorted(by: { $0.key > $1.key }) {
            log.append("\(index): \(state.debugDescription), ")
        }

        return log
    }
}

public extension [TrackedVoucher] {
    var toDebugDescription: String {
        "TrackedVouchers: \(count):" + map(\.debugDescription).joined(separator: ", ")
    }
}
