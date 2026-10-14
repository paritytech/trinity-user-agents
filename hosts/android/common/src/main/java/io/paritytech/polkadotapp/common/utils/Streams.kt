package io.paritytech.polkadotapp.common.utils

import java.io.InputStream

fun InputStream.readText() = bufferedReader().use { it.readText() }

/**
 * @see StreamsCompat.skipNBytes
 */
fun InputStream.skipNBytesCompat(numberOfBytesToSkip: Long) {
    return StreamsCompat.skipNBytes(this, numberOfBytesToSkip)
}

/**
 * @see StreamsCompat.readNBytes
 */
fun InputStream.readNBytesCompat(length: Int): ByteArray {
    return StreamsCompat.readNBytes(this, length)
}
