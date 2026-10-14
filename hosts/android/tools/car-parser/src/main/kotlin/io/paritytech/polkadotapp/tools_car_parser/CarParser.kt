package io.paritytech.polkadotapp.tools_car_parser

import com.google.protobuf.CodedInputStream
import io.paritytech.polkadotapp.tools_ipfs_api.Cid
import java.io.ByteArrayInputStream
import java.io.EOFException
import java.io.File
import java.io.InputStream
import java.io.RandomAccessFile

/**
 * Parses CARv1 (Content Addressable aRchive) files from IPFS.
 *
 * A CAR file is a binary archive that serializes content-addressed IPLD blocks
 * into a single sequential stream. It is the standard format for transferring
 * IPFS data as a single file (e.g., downloading a website archive from an IPFS gateway).
 *
 * ## Binary layout (CARv1)
 * ```
 * [header-varint | CBOR header] [block-varint | CID | block-data] [block-varint | CID | block-data] ...
 * ```
 * - **Header**: a varint for the byte length, followed by a DAG-CBOR encoded map
 *   containing `{ version: 1, roots: [<root CID>] }`.
 * - **Blocks**: each block is a varint (byte length of CID + data), followed by
 *   a raw CID, followed by the raw block bytes. Blocks are sequential with no index.
 *
 * ## Codecs
 * Each block's CID encodes a codec that determines how to interpret the block data:
 * - **dag-pb** (0x70): DAG-PB protobuf — contains UnixFS file/directory structure.
 * - **raw** (0x55): Raw bytes — the block data is the file content itself, not wrapped in protobuf.
 *
 * ## Reconstruction
 * After decoding, the blocks form a DAG (directed acyclic graph) where each block
 * is addressed by its CID. Starting from the root CID, [UnixFsDecoder] traverses
 * this DAG to reconstruct the original directory/file structure.
 *
 * @see UnixFsDecoder
 * @see CborHeaderDecoder
 */
object CarParser {
    /**
     * Parses a CARv1 archive and reconstructs the file tree.
     */
    fun parse(carBytes: ByteArray): Result<UnpackedCarArchive> = runCatching {
        val carFile = decode(carBytes)

        require(carFile.roots.isNotEmpty()) { "CAR archive has no root CIDs" }
        val rootCid = carFile.roots.first()

        val files = UnixFsDecoder.reconstructFileTree(rootCid, carFile.blocks)
        UnpackedCarArchive(files)
    }

    /**
     * Streams a CARv1 archive from disk and reconstructs the file tree, emitting each file to
     * [onFile] as it is reassembled rather than materializing the whole archive in memory.
     *
     * Unlike [parse], this never holds the archive (or any whole file) in memory: a single
     * sequential scan builds a `CID -> offset` index ([CarBlockIndex]), then a DFS traversal
     * streams each file's content straight from disk. Use this for large archives that would
     * OOM the buffered path.
     *
     * The [content] stream passed to [onFile] is backed by the open archive file and must be
     * fully consumed before [onFile] returns. Throws on malformed input (the caller is expected
     * to wrap this in `runCatching` / `mapCatching`); a thrown exception lets the caller clean up
     * any partially-written output.
     */
    fun unpack(carFile: File, onFile: (FilePath, content: InputStream) -> Unit) {
        buildIndex(carFile).use { index ->
            require(index.roots.isNotEmpty()) { "CAR archive has no root CIDs" }
            UnixFsDecoder.unpack(index.roots.first(), index, onFile)
        }
    }

    /**
     * Checks if the byte array starts with a valid CARv1 header.
     * Used to detect legacy deployments where the CID points to an uploaded CAR file
     * rather than a directory.
     */
    fun looksLikeCarArchive(data: ByteArray): Boolean {
        if (data.size < 10) return false

        return try {
            val input = ByteArrayInputStream(data)
            val codedInput = CodedInputStream.newInstance(input)
            val headerLength = codedInput.readRawVarint32()
            if (headerLength <= 0 || headerLength > data.size) return false

            val headerBytes = codedInput.readRawBytes(headerLength)
            val header = CborHeaderDecoder.decode(headerBytes)
            header.version == 1 && header.roots.isNotEmpty()
        } catch (_: Exception) {
            false
        }
    }

    private fun decode(carBytes: ByteArray): CarFile {
        val input = ByteArrayInputStream(carBytes)
        val codedInput = CodedInputStream.newInstance(input)

        val headerLength = codedInput.readRawVarint32()
        val headerBytes = codedInput.readRawBytes(headerLength)
        val header = CborHeaderDecoder.decode(headerBytes)

        val blocks = mutableMapOf<Cid, BlockData>()

        while (!codedInput.isAtEnd) {
            val blockLength = codedInput.readRawVarint32()
            val blockBytes = codedInput.readRawBytes(blockLength)

            val blockStream = ByteArrayInputStream(blockBytes)
            val cid = Cid.cast(blockStream as java.io.InputStream)
            val data = blockStream.readBytes()

            blocks[cid] = data
        }

        return CarFile(roots = header.roots, blocks = blocks)
    }

    /**
     * Scans a CAR file sequentially, recording each block's data offset and length into a
     * [CarBlockIndex] without copying block bytes. The returned index owns the open file handle.
     */
    private fun buildIndex(carFile: File): CarBlockIndex {
        val file = RandomAccessFile(carFile, "r")

        try {
            val headerLength = readRawVarint(file)
            val headerBytes = ByteArray(headerLength)
            file.readFully(headerBytes)
            val header = CborHeaderDecoder.decode(headerBytes)

            val blocks = HashMap<Cid, BlockRef>()
            val fileLength = file.length()

            while (file.filePointer < fileLength) {
                val blockLength = readRawVarint(file)
                val blockStart = file.filePointer

                // Read a bounded prefix to parse the CID, whose byte length is not known up front.
                val prefixLength = minOf(blockLength, MAX_CID_PREFIX_BYTES)
                val prefix = ByteArray(prefixLength)
                file.readFully(prefix)

                val prefixStream = ByteArrayInputStream(prefix)
                val cid = Cid.cast(prefixStream as InputStream)
                val cidLength = prefixLength - prefixStream.available()

                blocks[cid] = BlockRef(offset = blockStart + cidLength, length = blockLength - cidLength)
                file.seek(blockStart + blockLength)
            }

            return CarBlockIndex(file, header.roots, blocks)
        } catch (e: Throwable) {
            file.close()
            throw e
        }
    }

    private fun readRawVarint(file: RandomAccessFile): Int {
        var result = 0
        var shift = 0

        while (true) {
            val byte = file.read()
            if (byte < 0) throw EOFException("Unexpected end of CAR file while reading varint")

            result = result or ((byte and 0x7F) shl shift)
            if (byte and 0x80 == 0) return result

            shift += 7
            require(shift <= 35) { "Varint is too long" }
        }
    }

    private class CarFile(
        val roots: List<Cid>,
        val blocks: Map<Cid, BlockData>
    )

    // Comfortably larger than any IPFS CIDv1 (sha2-256 dag-pb/raw CIDs are ~36 bytes).
    private const val MAX_CID_PREFIX_BYTES = 256
}
