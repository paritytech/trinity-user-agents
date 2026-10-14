package io.paritytech.polkadotapp.database.model.chain

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Ignore
import androidx.room.Index
import io.paritytech.polkadotapp.common.utils.Identifiable

@Entity(
    tableName = "chain_assets",
    primaryKeys = ["chainId", "id"],
    foreignKeys = [
        ForeignKey(
            entity = ChainLocal::class,
            parentColumns = ["id"],
            childColumns = ["chainId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
    indices = [
        Index(value = ["chainId"])
    ]
)
data class ChainAssetLocal(
    val id: Int,
    val chainId: String,
    val name: String,
    val symbol: String,
    val priceId: String?,
    val precision: Int,
    val type: String?,
    val typeExtras: String?,
    val enabled: Boolean,
) : Identifiable {
    companion object {
        const val ENABLED_DEFAULT = true
    }

    @Ignore
    override val identifier: String = "$id:$chainId"
}

data class FullChainAssetIdLocal(val chainId: String, val assetId: Int)
