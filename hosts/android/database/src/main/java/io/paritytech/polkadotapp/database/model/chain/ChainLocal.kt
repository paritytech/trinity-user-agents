package io.paritytech.polkadotapp.database.model.chain

import androidx.room.Embedded
import androidx.room.Entity
import androidx.room.Ignore
import androidx.room.PrimaryKey
import io.paritytech.polkadotapp.common.utils.Identifiable

@Entity(tableName = "chains")
data class ChainLocal(
    @PrimaryKey val id: String,
    val genesisHash: String,
    val parentId: String?,
    val name: String,
    @Embedded
    val types: TypesConfig?,
    val prefix: Int,
    val isEthereumBased: Boolean,
    val isTestNet: Boolean,
    val hasSubstrateRuntime: Boolean,
    val additional: String?,
    val connectionState: ConnectionStateLocal,
    val nodeSelectionStrategy: NodeSelectionStrategyLocal,
) : Identifiable {
    enum class NodeSelectionStrategyLocal {
        ROUND_ROBIN,
        UNIFORM,
        UNKNOWN,
    }

    enum class ConnectionStateLocal {
        FULL_SYNC,
        LIGHT_SYNC,
        DISABLED,
    }

    @Ignore
    override val identifier: String = id

    data class TypesConfig(
        val url: String,
        val overridesCommon: Boolean,
    )
}
