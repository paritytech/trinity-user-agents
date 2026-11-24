package io.parity.qjsbench

import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** One inbound frame and the frames the worker must answer it with. */
class Step(val inbound: ByteArray, val outbound: List<ByteArray>)

/** `corpus.bin` as written by workloads/src/corpus-format.ts. */
fun loadCorpus(file: File): List<Step> {
    val buffer = ByteBuffer.wrap(file.readBytes()).order(ByteOrder.LITTLE_ENDIAN)
    fun take(len: Int) = ByteArray(len).also { buffer.get(it) }
    check(String(take(4)) == "QJSC") { "not a QJSC corpus" }
    check(buffer.int == 1) { "unsupported corpus version" }
    return List(buffer.int) {
        val inbound = take(buffer.int)
        val outbound = List(buffer.int) { take(buffer.int) }
        Step(inbound, outbound)
    }
}

/** Whether `frame` is the host starting a render: trait 17, method 0, Start. */
fun isRenderStart(frame: ByteArray): Boolean {
    if (frame.isEmpty()) return false
    val first = frame[0].toInt() and 0xff
    val (idLen, prefix) = when {
        first and 0b11 == 0 -> (first shr 2) to 1
        frame.size > 1 -> (((first shr 2) or ((frame[1].toInt() and 0xff) shl 6)) and 0x3fff) to 2
        else -> return false
    }
    val header = prefix + idLen
    return frame.size >= header + 3 &&
        frame[header].toInt() == 17 && frame[header + 1].toInt() == 0 && frame[header + 2].toInt() == 0
}

/** `(p50, p95)` of the samples. */
fun percentiles(samples: List<Double>): Pair<Double, Double> {
    val sorted = samples.sorted()
    fun at(fraction: Double) = sorted[minOf((sorted.size * fraction).toInt(), sorted.size - 1)]
    return at(0.5) to at(0.95)
}
