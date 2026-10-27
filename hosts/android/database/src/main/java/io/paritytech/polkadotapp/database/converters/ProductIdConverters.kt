package io.paritytech.polkadotapp.database.converters

import androidx.room.TypeConverter
import uniffi.truapi.ProductId

class ProductIdConverters {
    @TypeConverter
    fun encode(value: ProductId?): String? = value?.value

    @TypeConverter
    fun decode(value: String?): ProductId? = value?.let(ProductId::parse)
}
