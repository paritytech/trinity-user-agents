package io.paritytech.polkadotapp.database.converters

import androidx.room.TypeConverter

class IntListConverter {
    @TypeConverter
    fun fromIntList(list: List<Int>?): String? {
        return list?.joinToString(separator = ",")
    }

    @TypeConverter
    fun toIntList(value: String?): List<Int>? {
        if (value == null) return null
        if (value.isEmpty()) return emptyList()
        return value.split(',').map { it.toInt() }
    }
}
