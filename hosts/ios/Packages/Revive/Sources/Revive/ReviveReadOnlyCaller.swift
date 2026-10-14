import Foundation
import SubstrateSdk

/// The account read-only calls are evaluated as: pallet-revive's own (`modlpy/reviv`, zero-padded), so
/// no mapping or balance is needed to read a contract.
public enum ReviveReadOnlyCaller {
    public static let accountId: AccountId = {
        let prefix = Data("modlpy/reviv".utf8)
        return prefix + Data(repeating: 0, count: 32 - prefix.count)
    }()
}
