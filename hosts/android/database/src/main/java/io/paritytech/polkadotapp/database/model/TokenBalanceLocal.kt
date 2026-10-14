package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.ForeignKey
import io.paritytech.polkadotapp.database.model.chain.ChainAssetLocal
import java.math.BigInteger

@Entity(
    tableName = "tokenBalances",
    primaryKeys = ["assetId", "chainId", "metaId"],
    foreignKeys = [
        ForeignKey(
            entity = ChainAssetLocal::class,
            parentColumns = ["id", "chainId"],
            childColumns = ["assetId", "chainId"],
            onDelete = ForeignKey.CASCADE
        )
    ]
)
data class TokenBalanceLocal(
    val assetId: Int,
    val chainId: String,
    @ColumnInfo(index = true) val metaId: Long,
    val freeInPlanks: BigInteger,
    val frozenInPlanks: BigInteger,
    val reservedInPlanks: BigInteger,
    val transferableMode: TransferableModeLocal,
    val edCountingMode: EDCountingModeLocal,
) {
    companion object {
        fun defaultTransferableMode(): TransferableModeLocal = TransferableModeLocal.LEGACY

        fun defaultEdCountingMode(): EDCountingModeLocal = EDCountingModeLocal.TOTAL

        fun createEmpty(
            assetId: Int,
            chainId: String,
            metaId: Long
        ) = TokenBalanceLocal(
            assetId = assetId,
            chainId = chainId,
            metaId = metaId,
            freeInPlanks = BigInteger.ZERO,
            reservedInPlanks = BigInteger.ZERO,
            transferableMode = defaultTransferableMode(),
            edCountingMode = defaultEdCountingMode(),
            frozenInPlanks = BigInteger.ZERO,
        )
    }

    enum class TransferableModeLocal {
        LEGACY, HOLDS_AND_FREEZES
    }

    enum class EDCountingModeLocal {
        TOTAL, FREE
    }
}
