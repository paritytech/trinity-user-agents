package io.paritytech.polkadotapp.database.model.chain

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Ignore
import androidx.room.Index
import io.paritytech.polkadotapp.common.utils.Identifiable

@Entity(
    tableName = "chain_explorers",
    primaryKeys = ["chainId", "name"],
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
data class ChainExplorerLocal(
    val chainId: String,
    val name: String,
    val extrinsic: String?,
    val account: String?,
    val event: String?,
) : Identifiable {
    @Ignore
    override val identifier: String = "$chainId:$name"
}
