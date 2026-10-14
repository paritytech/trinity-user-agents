package io.paritytech.polkadotapp.chains.network.binding

import io.paritytech.polkadotapp.common.data.substrate.cast
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.orZero
import java.math.BigInteger

fun bindNumber(dynamicInstance: Any?): BigInteger = dynamicInstance.cast()

fun bindNumberOrNull(dynamicInstance: Any?): BigInteger? = dynamicInstance?.cast()

fun bindInt(dynamicInstance: Any?): Int = bindNumber(dynamicInstance).toInt()

fun bindNumberOrZero(dynamicInstance: Any?): BigInteger = dynamicInstance?.let(::bindNumber).orZero()

fun bindString(dynamicInstance: Any?): String = dynamicInstance.cast<ByteArray>().decodeToString()

fun bindBoolean(dynamicInstance: Any?): Boolean = dynamicInstance.cast()

fun bindByteArray(dynamicInstance: Any?): ByteArray = dynamicInstance.cast()

fun bindDataByteArray(dynamicInstance: Any?): DataByteArray = dynamicInstance.cast<ByteArray>().toDataByteArray()

fun bindAnyOrNull(dynamicInstance: Any?): Any? = dynamicInstance?.cast()
