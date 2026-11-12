package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.pow

/**
 * Reads a Radiance `.hdr` face of the prefiltered studio environment.
 *
 * The studio ships as HDR rather than as an ordinary image because it holds light, not colour: the bright
 * parts of a softbox run well past 1 and are what a polished coin actually reflects. There is no system
 * decoder for the format, so this is the reference host's reader, which handles the run-length encoded
 * scanlines the exporter writes.
 */
object CoinageRadianceImage {
    /** RGBA half floats, alpha always 1, ready to upload to an `RGBA16F` texture. */
    class Face(val width: Int, val height: Int, val pixels: ByteBuffer)

    class Malformed(message: String) : IllegalStateException(message)

    /**
     * One throw site for the whole decoder. Every check here is "the file ends where it should not",
     * and they read better as guards than as a ladder of `throw`.
     */
    private fun malformed(what: String): Nothing = throw Malformed(what)

    fun decode(bytes: ByteArray): Face {
        var cursor = 0

        fun line(): String {
            val start = cursor

            while (cursor < bytes.size && bytes[cursor] != NEWLINE) cursor++

            if (cursor >= bytes.size) malformed("unterminated header")

            return String(bytes, start, cursor - start, Charsets.US_ASCII).also { cursor++ }
        }

        while (line().isNotEmpty()) Unit

        val dimensions = line().split(" ")
        val height = dimensions.getOrNull(1)?.toIntOrNull()
        val width = dimensions.getOrNull(3)?.toIntOrNull()

        if (dimensions.size < 4 || height == null || width == null) malformed("no resolution line")

        val pixels = ByteBuffer.allocateDirect(width * height * 4 * 2).order(ByteOrder.nativeOrder())
        val scanline = ByteArray(width * 4)

        for (row in 0 until height) {
            cursor = read(bytes, cursor, scanline, width)
            convert(scanline, pixels, row, width)
        }

        pixels.rewind()

        return Face(width, height, pixels)
    }

    /** Adaptive run-length encoding, one colour channel at a time across the row. */
    private fun read(bytes: ByteArray, start: Int, scanline: ByteArray, width: Int): Int {
        var cursor = start

        if (cursor + 4 > bytes.size) malformed("truncated scanline")

        val isAdaptive = width >= 8 && width < 32_768 &&
            bytes[cursor] == RLE_MARKER && bytes[cursor + 1] == RLE_MARKER

        if (!isAdaptive) {
            if (cursor + width * 4 > bytes.size) malformed("truncated row")

            System.arraycopy(bytes, cursor, scanline, 0, width * 4)

            return cursor + width * 4
        }

        cursor += 4

        for (channel in 0 until 4) {
            cursor = readChannel(bytes, cursor, scanline, width, channel)
        }

        return cursor
    }

    private fun readChannel(
        bytes: ByteArray,
        start: Int,
        scanline: ByteArray,
        width: Int,
        channel: Int
    ): Int {
        var cursor = start
        var column = 0

        while (column < width) {
            if (cursor >= bytes.size) malformed("truncated run")

            var count = bytes[cursor].toInt() and 0xFF
            cursor++
            val isRun = count > 128

            if (isRun) count -= 128

            if (isRun && cursor >= bytes.size) malformed("truncated run value")

            val repeated = if (isRun) bytes[cursor] else 0

            if (isRun) cursor++

            var taken = 0

            while (taken < count && column < width) {
                if (!isRun && cursor >= bytes.size) malformed("truncated literal")

                scanline[column * 4 + channel] = if (isRun) repeated else bytes[cursor]

                if (!isRun) cursor++

                column++
                taken++
            }
        }

        return cursor
    }

    /**
     * Radiance packs a shared exponent in the alpha byte: the mantissa bytes are scaled by
     * `2^(exponent - 136)`, the half-step keeping the quantisation unbiased.
     */
    private fun convert(scanline: ByteArray, pixels: ByteBuffer, row: Int, width: Int) {
        for (column in 0 until width) {
            val exponent = scanline[column * 4 + 3].toInt() and 0xFF
            val scale = if (exponent > 0) 2f.pow(exponent - RADIANCE_BIAS) else 0f
            val offset = (row * width + column) * 4 * 2

            for (channel in 0 until 3) {
                val mantissa = scanline[column * 4 + channel].toInt() and 0xFF
                val value = if (exponent > 0) (mantissa + 0.5f) * scale else 0f

                pixels.putShort(offset + channel * 2, halfOf(value))
            }

            pixels.putShort(offset + 6, ONE_HALF)
        }
    }

    /**
     * IEEE binary16, rounded toward zero. Written out rather than taken from `Float.floatToHalf`, which
     * arrives in API 34 and this app still supports older.
     */
    private fun halfOf(value: Float): Short {
        val bits = java.lang.Float.floatToRawIntBits(value)
        val sign = (bits ushr 16) and 0x8000
        val exponent = ((bits ushr 23) and 0xFF) - 127 + 15
        val mantissa = (bits ushr 13) and 0x3FF

        return when {
            exponent <= 0 -> sign.toShort()
            exponent >= 0x1F -> (sign or 0x7BFF).toShort()
            else -> (sign or (exponent shl 10) or mantissa).toShort()
        }
    }

    private const val NEWLINE: Byte = 10
    private const val RLE_MARKER: Byte = 2
    private const val RADIANCE_BIAS = 136
    private val ONE_HALF: Short = 0x3C00
}
