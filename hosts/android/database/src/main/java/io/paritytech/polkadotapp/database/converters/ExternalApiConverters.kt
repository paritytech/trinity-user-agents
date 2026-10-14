package io.paritytech.polkadotapp.database.converters

import androidx.room.TypeConverter
import io.paritytech.polkadotapp.common.utils.enumValueOfOrNull
import io.paritytech.polkadotapp.database.model.chain.ChainExternalApiLocal.ApiType

class ExternalApiConverters {
    @TypeConverter
    fun fromApiType(apiType: ApiType): String {
        return apiType.name
    }

    @TypeConverter
    fun toApiType(raw: String): ApiType {
        return enumValueOfOrNull<ApiType>(raw) ?: ApiType.UNKNOWN
    }
}
