package io.paritytech.polkadotapp.tools_car_parser

import io.paritytech.polkadotapp.common.utils.buildByteArray
import io.paritytech.polkadotapp.tools_car_parser.proto.UnixFsProto
import io.paritytech.polkadotapp.tools_ipfs_api.Cid
import java.io.ByteArrayInputStream
import java.io.InputStream
import java.io.SequenceInputStream
import java.util.Enumeration

typealias FilePath = String

/**
 * Decodes UnixFS protobuf data and reconstructs file trees from IPLD block maps.
 *
 * UnixFS is a protobuf schema (see `unixfs.proto`) embedded inside DAG-PB nodes
 * that gives file system semantics (files, directories, symlinks) to the raw
 * linked DAG structure.
 *
 * ## Block codecs
 * Each block's CID carries a codec:
 * - **dag-pb** (0x70): DAG-PB protobuf with embedded UnixFS — parsed for structure.
 * - **raw** (0x55): Raw bytes — the block data is the file content itself.
 *
 * ## File tree reconstruction algorithm
 *
 * Starting from the root CID, the decoder performs a DFS traversal:
 *
 * 1. **Look up** the block by CID in the block map.
 * 2. **Check codec**: if raw, emit the data directly as file content.
 * 3. **Decode DAG-PB** to get the embedded UnixFS data and child links.
 * 4. **Branch by UnixFS type**:
 *    - *Directory*: recurse into each link, appending the link name to the path.
 *    - *Leaf*: emit the inline content at the current path.
 *    - *ChunkedFile*: follow the links in order, concatenating each child's data
 *      to reassemble the original file. Children may themselves be chunked (nested).
 *
 * @see DagPbDecoder
 */
object UnixFsDecoder {
    fun reconstructFileTree(
        rootCid: Cid,
        blocks: Map<Cid, BlockData>
    ): Map<FilePath, FileContent> {
        val result = mutableMapOf<FilePath, FileContent>()
        traverseNode(rootCid, "", blocks, result)
        return result
    }

    /**
     * Streaming counterpart of [reconstructFileTree]: performs the same DFS but, instead of
     * collecting file content into a map, emits each file to [onFile] as an [InputStream] backed
     * by the on-disk [index]. Memory stays bounded — only small DAG-PB nodes and the chunk-order
     * list are held; file bytes are pulled from disk lazily as each stream is consumed.
     */
    internal fun unpack(
        rootCid: Cid,
        index: CarBlockIndex,
        onFile: (FilePath, content: InputStream) -> Unit
    ) {
        traverseNodeStreaming(rootCid, "", index, onFile)
    }

    private fun traverseNodeStreaming(
        cid: Cid,
        currentPath: FilePath,
        index: CarBlockIndex,
        onFile: (FilePath, content: InputStream) -> Unit
    ) {
        // Raw codec blocks contain file content directly — stream the region straight from disk.
        if (cid.codec == Cid.Codec.Raw) {
            onFile(currentPath, index.openRegion(index.blockRef(cid)))
            return
        }

        val dagPbNode = DagPbDecoder.decode(index.readBlock(cid))

        require(dagPbNode.data != null || dagPbNode.links.isNotEmpty()) {
            "Empty DAG-PB node (no data, no links) at path: $currentPath"
        }

        val unixFs = dagPbNode.data?.let { decodeUnixFs(it) }

        when (unixFs) {
            is UnixFsData.Directory, null -> {
                for (link in dagPbNode.links) {
                    val linkName = requireNotNull(link.name) { "Directory entry is missing a name at path: $currentPath" }
                    traverseNodeStreaming(link.hash, "$currentPath/$linkName", index, onFile)
                }
            }
            is UnixFsData.Leaf -> {
                // Inline content lives in the node itself (bounded by one block); wrap as-is.
                onFile(currentPath, ByteArrayInputStream(unixFs.content))
            }
            is UnixFsData.ChunkedFile -> {
                val segments = mutableListOf<() -> InputStream>()
                collectChunkSegments(dagPbNode, index, segments)
                onFile(currentPath, SequenceInputStream(LazyStreamEnumeration(segments)))
            }
        }
    }

    /**
     * Walks a chunked file's DAG-PB link tree in order, appending a lazy stream producer per leaf
     * chunk. Raw leaves stream their region from disk; inline (UnixFS leaf) chunks are decoded one
     * at a time when their producer is invoked, so at most one chunk is in memory at any moment.
     */
    private fun collectChunkSegments(
        dagPbNode: DagPbNode,
        index: CarBlockIndex,
        out: MutableList<() -> InputStream>
    ) {
        for (link in dagPbNode.links) {
            if (link.hash.codec == Cid.Codec.Raw) {
                val ref = index.blockRef(link.hash)
                out.add { index.openRegion(ref) }
                continue
            }

            val childDagPb = DagPbDecoder.decode(index.readBlock(link.hash))

            when {
                childDagPb.links.isNotEmpty() -> collectChunkSegments(childDagPb, index, out)
                childDagPb.data?.let { decodeUnixFs(it) } is UnixFsData.Leaf -> {
                    val chunkCid = link.hash
                    out.add { ByteArrayInputStream(leafChunkContent(chunkCid, index)) }
                }
                else -> error("Unexpected chunk child: expected Leaf or nested chunks at CID ${link.hash}")
            }
        }
    }

    private fun leafChunkContent(cid: Cid, index: CarBlockIndex): FileContent {
        val dagPb = DagPbDecoder.decode(index.readBlock(cid))
        val leaf = dagPb.data?.let { decodeUnixFs(it) } as? UnixFsData.Leaf
            ?: error("Expected UnixFS leaf chunk at CID $cid")
        return leaf.content
    }

    private fun traverseNode(
        cid: Cid,
        currentPath: FilePath,
        blocks: Map<Cid, BlockData>,
        result: MutableMap<FilePath, FileContent>
    ) {
        val blockData = blocks[cid]
            ?: throw IllegalStateException("Block not found for CID: $cid")

        // Raw codec blocks contain file content directly, not DAG-PB protobuf
        if (cid.codec == Cid.Codec.Raw) {
            result[currentPath] = blockData
            return
        }

        val dagPbNode = DagPbDecoder.decode(blockData)

        require(dagPbNode.data != null || dagPbNode.links.isNotEmpty()) {
            "Empty DAG-PB node (no data, no links) at path: $currentPath"
        }

        val unixFs = dagPbNode.data?.let { decodeUnixFs(it) }

        when (unixFs) {
            // A node with no UnixFS data but with links is treated as a directory.
            // This is valid in IPFS — some directory nodes omit the UnixFS wrapper.
            is UnixFsData.Directory, null -> {
                for (link in dagPbNode.links) {
                    val linkName = requireNotNull(link.name) { "Directory entry is missing a name at path: $currentPath" }
                    traverseNode(link.hash, "$currentPath/$linkName", blocks, result)
                }
            }
            is UnixFsData.Leaf -> {
                result[currentPath] = unixFs.content
            }
            is UnixFsData.ChunkedFile -> {
                result[currentPath] = assembleChunkedFile(dagPbNode, blocks)
            }
        }
    }

    /**
     * Reassembles a chunked file by following DAG-PB links in order.
     * Each link points to a child block that is either a leaf chunk (raw or DAG-PB)
     * or another level of chunking (for very large files).
     */
    private fun assembleChunkedFile(
        dagPbNode: DagPbNode,
        blocks: Map<Cid, BlockData>
    ): FileContent = buildByteArray {
        for (link in dagPbNode.links) {
            val block = blocks[link.hash]
                ?: throw IllegalStateException("Chunk block not found for CID: ${link.hash}")

            // Raw codec blocks are leaf chunks — use data directly
            if (link.hash.codec == Cid.Codec.Raw) {
                write(block)
                continue
            }

            val childDagPb = DagPbDecoder.decode(block)
            val childUnixFs = childDagPb.data?.let { decodeUnixFs(it) }

            when {
                childDagPb.links.isNotEmpty() -> write(assembleChunkedFile(childDagPb, blocks))
                childUnixFs is UnixFsData.Leaf -> write(childUnixFs.content)
                else -> error("Unexpected chunk child: expected Leaf or nested chunks, got $childUnixFs")
            }
        }
    }

    private fun decodeUnixFs(data: BlockData): UnixFsData {
        val proto = UnixFsProto.Data.parseFrom(data)

        return when (proto.type) {
            UnixFsProto.Data.DataType.Directory -> UnixFsData.Directory

            UnixFsProto.Data.DataType.Raw -> {
                require(proto.hasData()) { "UnixFS Raw node is missing data" }
                UnixFsData.Leaf(content = proto.data.toByteArray())
            }

            else -> {
                val isChunked = proto.blocksizesCount > 0
                if (isChunked) {
                    UnixFsData.ChunkedFile
                } else {
                    require(proto.hasData()) { "UnixFS File leaf node is missing data" }
                    UnixFsData.Leaf(content = proto.data.toByteArray())
                }
            }
        }
    }
}

private sealed interface UnixFsData {
    data object Directory : UnixFsData

    data object ChunkedFile : UnixFsData

    class Leaf(val content: FileContent) : UnixFsData
}

/**
 * An [Enumeration] over stream producers that invokes each producer only when reached, so
 * [SequenceInputStream] opens (and the producer materializes) one chunk stream at a time.
 */
private class LazyStreamEnumeration(
    private val producers: List<() -> InputStream>
) : Enumeration<InputStream> {
    private var index = 0

    override fun hasMoreElements(): Boolean = index < producers.size

    override fun nextElement(): InputStream = producers[index++].invoke()
}
