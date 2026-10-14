package io.paritytech.polkadotapp.database.converters

import androidx.room.TypeConverter
import io.paritytech.polkadotapp.common.utils.enumValueOfOrNull
import io.paritytech.polkadotapp.database.model.chain.ChainLocal.ConnectionStateLocal
import io.paritytech.polkadotapp.database.model.chain.ChainLocal.NodeSelectionStrategyLocal

class ChainConverters {
    @TypeConverter
    fun fromNodeStrategy(strategy: NodeSelectionStrategyLocal): String = strategy.name

    @TypeConverter
    fun toNodeStrategy(name: String): NodeSelectionStrategyLocal {
        return enumValueOfOrNull<NodeSelectionStrategyLocal>(name) ?: NodeSelectionStrategyLocal.UNKNOWN
    }

    @TypeConverter
    fun fromConnection(connectionState: ConnectionStateLocal): String = connectionState.name

    @TypeConverter
    fun toConnection(name: String): ConnectionStateLocal {
        return enumValueOfOrNull<ConnectionStateLocal>(name) ?: ConnectionStateLocal.LIGHT_SYNC
    }
}
