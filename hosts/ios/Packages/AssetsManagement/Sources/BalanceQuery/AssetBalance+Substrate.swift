import BigInt
import Foundation
import SubstrateSdk

public extension AssetBalance {
    init(
        accountInfo: SystemPallet.AccountInfo?,
        chainAssetId: ChainAssetId,
        accountId: AccountId
    ) {
        self.init(
            chainAssetId: chainAssetId,
            accountId: accountId,
            freeInPlank: accountInfo?.data.free ?? 0,
            reservedInPlank: accountInfo?.data.reserved ?? 0,
            frozenInPlank: accountInfo?.data.locked ?? 0,
            edCountMode: accountInfo?.data.edCountMode ?? .basedOnFree,
            transferrableMode: accountInfo?.data.transferrableModel ?? .regular,
            blocked: false
        )
    }

    init(
        ormlAccount: OrmlAccount?,
        chainAssetId: ChainAssetId,
        accountId: AccountId
    ) {
        self.init(
            chainAssetId: chainAssetId,
            accountId: accountId,
            freeInPlank: ormlAccount?.free ?? 0,
            reservedInPlank: ormlAccount?.reserved ?? 0,
            frozenInPlank: ormlAccount?.frozen ?? 0,
            edCountMode: .basedOnTotal,
            transferrableMode: .regular,
            blocked: false
        )
    }

    init(
        assetsAccount: AssetsPallet.Account?,
        assetsDetails: AssetsPallet.Details?,
        chainAssetId: ChainAssetId,
        accountId: AccountId
    ) {
        let balance = assetsAccount?.balance ?? 0

        let isFrozen = (assetsAccount?.isFrozen ?? false) || (assetsDetails?.isFrozen ?? false)
        let isBlocked = assetsAccount?.isBlocked ?? false

        self.init(
            chainAssetId: chainAssetId,
            accountId: accountId,
            freeInPlank: balance,
            reservedInPlank: 0,
            frozenInPlank: isFrozen ? balance : 0,
            edCountMode: .basedOnTotal,
            transferrableMode: .regular,
            blocked: isBlocked
        )
    }
}

extension SystemPallet.AccountData {
    static let fungibleTraitLogic = BigUInt(1) << 127

    var isFungibleTraitLogic: Bool {
        guard let flags else {
            return false
        }

        return (flags & Self.fungibleTraitLogic) == Self.fungibleTraitLogic
    }

    var edCountMode: AssetBalance.ExistentialDepositCountMode {
        isFungibleTraitLogic ? .basedOnFree : .basedOnTotal
    }

    var transferrableModel: AssetBalance.TransferrableMode {
        isFungibleTraitLogic ? .fungibleTrait : .regular
    }
}
