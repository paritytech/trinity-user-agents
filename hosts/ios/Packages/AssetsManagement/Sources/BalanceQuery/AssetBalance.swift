import BigInt
import Foundation
import SubstrateSdk

public struct AssetBalance: Equatable {
    public let chainAssetId: ChainAssetId
    public let accountId: AccountId
    public let freeInPlank: BigUInt
    public let reservedInPlank: BigUInt
    public let frozenInPlank: BigUInt
    public let edCountMode: ExistentialDepositCountMode
    public let transferrableMode: TransferrableMode
    public let blocked: Bool

    public init(
        chainAssetId: ChainAssetId,
        accountId: AccountId,
        freeInPlank: BigUInt,
        reservedInPlank: BigUInt,
        frozenInPlank: BigUInt,
        edCountMode: ExistentialDepositCountMode,
        transferrableMode: TransferrableMode,
        blocked: Bool
    ) {
        self.chainAssetId = chainAssetId
        self.accountId = accountId
        self.freeInPlank = freeInPlank
        self.reservedInPlank = reservedInPlank
        self.frozenInPlank = frozenInPlank
        self.edCountMode = edCountMode
        self.transferrableMode = transferrableMode
        self.blocked = blocked
    }

    public var totalInPlank: BigUInt { freeInPlank + reservedInPlank }

    public var isZero: Bool {
        [freeInPlank, reservedInPlank, frozenInPlank].allSatisfy { $0 == 0 }
    }

    public var transferable: BigUInt {
        Self.transferrableBalance(
            from: freeInPlank,
            frozen: frozenInPlank,
            reserved: reservedInPlank,
            mode: transferrableMode
        )
    }

    public var locked: BigUInt {
        totalInPlank > transferable ? totalInPlank - transferable : 0
    }

    public var balanceCountingEd: BigUInt {
        switch edCountMode {
        case .basedOnTotal:
            totalInPlank
        case .basedOnFree:
            freeInPlank
        }
    }

    public func newTransferable(for frozen: BigUInt) -> BigUInt {
        Self.transferrableBalance(
            from: freeInPlank,
            frozen: frozen,
            reserved: reservedInPlank,
            mode: transferrableMode
        )
    }

    public func regularTransferrableBalance() -> BigUInt {
        Self.transferrableBalance(
            from: freeInPlank,
            frozen: frozenInPlank,
            reserved: reservedInPlank,
            mode: .regular
        )
    }

    public static func transferrableBalance(
        from free: BigUInt,
        frozen: BigUInt,
        reserved: BigUInt,
        mode: TransferrableMode
    ) -> BigUInt {
        switch mode {
        case .regular:
            return free > frozen ? free - frozen : 0
        case .fungibleTrait:
            let locked = frozen > reserved ? frozen - reserved : 0
            return free > locked ? free - locked : 0
        }
    }
}

public extension AssetBalance {
    enum ExistentialDepositCountMode {
        case basedOnTotal
        case basedOnFree
    }

    enum TransferrableMode {
        case regular
        case fungibleTrait
    }
}

public extension AssetBalance {
    static func createIdentifier(
        for chainAssetId: ChainAssetId,
        accountId: AccountId
    ) -> String {
        let data = (chainAssetId.stringValue + "-\(accountId.toHex())").data(using: .utf8)!
        return data.sha256().toHex()
    }

    var identifier: String { Self.createIdentifier(for: chainAssetId, accountId: accountId) }

    static func createZero(
        for chainAssetId: ChainAssetId,
        accountId: AccountId
    ) -> AssetBalance {
        AssetBalance(
            chainAssetId: chainAssetId,
            accountId: accountId,
            freeInPlank: 0,
            reservedInPlank: 0,
            frozenInPlank: 0,
            edCountMode: .basedOnFree,
            transferrableMode: .regular,
            blocked: false
        )
    }
}
