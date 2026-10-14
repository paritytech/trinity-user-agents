package io.paritytech.polkadotapp.database.model.chain

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Ignore
import androidx.room.Index
import io.paritytech.polkadotapp.common.utils.Identifiable

@Entity(
    tableName = "chain_external_apis",
    primaryKeys = ["chainId", "url", "apiType"],
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
data class ChainExternalApiLocal(
    val chainId: String,
    val apiType: ApiType,
    val parameters: String?,
    val url: String,
) : Identifiable {
    enum class ApiType {
        UNKNOWN,
        HOP
    }

    @Ignore
    override val identifier: String = "$chainId:$url:$apiType"
}
