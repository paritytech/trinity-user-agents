package io.paritytech.polkadotapp.common.utils

import java.io.ByteArrayOutputStream

fun ByteArray.dropBytes(count: Int) = copyOfRange(count, size)

fun ByteArray.chunkedSequence(chunkSize: Int): Sequence<ByteArray> =
    (indices step chunkSize).asSequence().map { start -> copyOfRange(start, minOf(start + chunkSize, size)) }

fun ByteArray.compareTo(other: ByteArray, unsigned: Boolean): Int {
    if (size != other.size) {
        return size - other.size
    }

    for (i in 0 until size) {
        val result = if (unsigned) {
            this[i].toUByte().compareTo(other[i].toUByte())
        } else {
            this[i].compareTo(other[i])
        }

        if (result != 0) {
            return result
        }
    }

    return 0
}

fun ByteArray.startsWith(prefix: ByteArray): Boolean {
    if (prefix.size > size) return false

    prefix.forEachIndexed { index, byte ->
        if (get(index) != byte) return false
    }

    return true
}

fun ByteArray.endsWith(suffix: ByteArray): Boolean {
    if (suffix.size > size) return false

    val offset = size - suffix.size

    suffix.forEachIndexed { index, byte ->
        if (get(offset + index) != byte) return false
    }

    return true
}

fun ByteArray.padEnd(expectedSize: Int, padding: Byte = 0): ByteArray {
    if (size >= expectedSize) return this

    val padded = ByteArray(expectedSize) { padding }
    return copyInto(padded)
}

fun buildByteArray(use: ByteArrayOutputStream.() -> Unit): ByteArray {
    return ByteArrayOutputStream().apply(use).toByteArray()
}
