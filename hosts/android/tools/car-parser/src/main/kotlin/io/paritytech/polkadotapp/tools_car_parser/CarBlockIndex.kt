package io.paritytech.polkadotapp.tools_car_parser

import io.paritytech.polkadotapp.tools_ipfs_api.Cid
import java.io.Closeable
import java.io.InputStream
import java.io.RandomAccessFile

/**
 * Location of a single block's data region within a CAR file on disk.
 *
 * [offset] points past the block's CID, at the first byte of the block data;
 * [length] is the data byte count. Holding offsets instead of the bytes themselves
 * is what keeps the streaming unpack path's memory bounded regardless of archive size.
 */
internal class BlockRef(
    val offset: Long,
    val length: Int
)

/**
 * A CAR archive backed by an on-disk file plus an in-memory `CID -> [BlockRef]` index.
 *
 * Built by [CarParser.unpack] in a single sequential scan that records block offsets without
 * copying block bytes. Block data is then read back on demand — small DAG-PB nodes fully via
 * [readBlock], file content lazily via [openRegion] — so only offsets and one transient block
 * live in memory at a time.
 *
 * Owns the open [RandomAccessFile]; callers must [close] it. Not thread-safe: all reads seek the
 * shared file handle, so the streams handed out must be consumed sequentially (which the
 * single-threaded DFS traversal guarantees).
 */
internal class CarBlockIndex(
    private val file: RandomAccessFile,
    val roots: List<Cid>,
    private val blocks: Map<Cid, BlockRef>
) : Closeable {
    fun blockRef(cid: Cid): BlockRef =
        blocks[cid] ?: throw IllegalStateException("Block not found for CID: $cid")

    /** Reads a whole block into memory — intended for small DAG-PB nodes, not raw file content. */
    fun readBlock(cid: Cid): ByteArray {
        val ref = blockRef(cid)
        val buffer = ByteArray(ref.length)
        file.seek(ref.offset)
        file.readFully(buffer)
        return buffer
    }

    /** Opens a stream over a block's data region; reads are pulled lazily from disk. */
    fun openRegion(ref: BlockRef): InputStream = RegionInputStream(file, ref.offset, ref.length)

    override fun close() = file.close()

    private class RegionInputStream(
        private val file: RandomAccessFile,
        private val start: Long,
        private val length: Int
    ) : InputStream() {
        private var position = 0

        override fun read(): Int {
            if (position >= length) return -1
            file.seek(start + position)
            val byte = file.read()
            if (byte >= 0) position++
            return byte
        }

        override fun read(buffer: ByteArray, offset: Int, count: Int): Int {
            if (position >= length) return -1
            file.seek(start + position)
            val toRead = minOf(count, length - position)
            val read = file.read(buffer, offset, toRead)
            if (read > 0) position += read
            return read
        }

        override fun available(): Int = length - position
    }
}
