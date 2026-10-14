package io.paritytech.polkadotapp.common.utils.scale

import io.emeraldpay.polkaj.scale.ScaleCodecReader
import io.emeraldpay.polkaj.scale.ScaleCodecWriter
import io.novasama.substrate_sdk_android.scale.NonNullFieldDelegate
import io.novasama.substrate_sdk_android.scale.Schema
import io.novasama.substrate_sdk_android.scale.dataType.DataType

object EmptyDataType : DataType<Unit>() {
    override fun read(reader: ScaleCodecReader) = Unit

    override fun write(writer: ScaleCodecWriter, value: Unit) = Unit

    override fun conformsType(value: Any?) = value is Unit
}

fun <S : Schema<S>> S.empty() = NonNullFieldDelegate(EmptyDataType, this, Unit)
