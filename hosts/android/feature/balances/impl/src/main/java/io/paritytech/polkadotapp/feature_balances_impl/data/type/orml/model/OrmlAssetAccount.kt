package io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.database.model.TokenBalanceLocal
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.TransferableMode
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.legacy
import kotlinx.serialization.Serializable

@Serializable
internal data class OrmlAssetAccount(
    val free: Balance,
    val reserved: Balance,
    val frozen: Balance
) {
    companion object {
        fun empty() = OrmlAssetAccount(
            free = Balance.ZERO,
            reserved = Balance.ZERO,
            frozen = Balance.ZERO
        )
    }
}

internal fun OrmlAssetAccount.toLocalTokenBalance(
    chainAsset: Chain.Asset,
    metaId: Long,
): TokenBalanceLocal {
    return TokenBalanceLocal(
        assetId = chainAsset.id,
        chainId = chainAsset.chainId,
        metaId = metaId,
        freeInPlanks = free.value,
        frozenInPlanks = frozen.value,
        reservedInPlanks = reserved.value,
        transferableMode = TokenBalanceLocal.TransferableModeLocal.LEGACY,
        edCountingMode = TokenBalanceLocal.EDCountingModeLocal.TOTAL,
    )
}

internal fun OrmlAssetAccount.toTokenBalance(
    chainAsset: Chain.Asset,
): TokenBalance {
    return TokenBalance.legacy(
        token = chainAsset,
        free = free,
        frozen = frozen,
        reserved = reserved,
    )
}

internal fun OrmlAssetAccount?.orEmpty(): OrmlAssetAccount {
    return this ?: OrmlAssetAccount.empty()
}
internal fun OrmlAssetAccount.transferableBalance(): Balance {
    return TransferableMode.LEGACY.transferableBalance(free, frozen, reserved)
}
