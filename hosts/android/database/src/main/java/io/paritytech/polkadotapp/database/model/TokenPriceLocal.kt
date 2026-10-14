package io.paritytech.polkadotapp.database.model

import androidx.room.Embedded
import androidx.room.Entity
import androidx.room.ForeignKey
import io.paritytech.polkadotapp.database.model.chain.ChainAssetLocal
import io.paritytech.polkadotapp.database.model.chain.FullChainAssetIdLocal
import java.math.BigDecimal

@Entity(
    tableName = "tokenPrices",
    primaryKeys = ["assetId", "chainId", "currencyId"],
    foreignKeys = [
        ForeignKey(
            entity = ChainAssetLocal::class,
            parentColumns = ["id", "chainId"],
            childColumns = ["assetId", "chainId"],
            onDelete = ForeignKey.CASCADE
        )
    ]
)
data class TokenPriceLocal(
    @Embedded
    val assetId: FullChainAssetIdLocal,
    val currencyId: Int,
    val price: BigDecimal?,
)
