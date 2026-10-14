package io.paritytech.polkadotapp.tools_car_parser

import io.paritytech.polkadotapp.tools_car_parser.proto.MerkleDagProto.PBNode
import io.paritytech.polkadotapp.tools_ipfs_api.Cid

typealias BlockData = ByteArray
typealias FileContent = ByteArray

/**
 * Domain model for a decoded DAG-PB node.
 *
 * [data] contains the embedded UnixFS protobuf (if present).
 * [links] point to child nodes (files in a directory, or chunks of a large file).
 */
class DagPbNode(
    val data: BlockData?,
    val links: List<DagPbLink>
)

/**
 * A reference to a child node in a DAG-PB structure.
 *
 * [name] is present for directory entries but null for chunk links in chunked files.
 * [size] is optional metadata about the cumulative size of the linked subtree.
 */
data class DagPbLink(
    val hash: Cid,
    val name: String?,
    val size: Long?
)

/**
 * Decodes DAG-PB (MerkleDAG Protobuf) blocks — the standard IPLD codec
 * used by IPFS to represent file system structures.
 *
 * DAG-PB is a thin protobuf wrapper (see `merkledag.proto`) with two fields:
 * - **Data** (bytes): an embedded UnixFS protobuf describing the node type and file content.
 * - **Links** (repeated): references to child nodes, each with a CID hash, a name, and a size.
 *
 * DAG-PB itself has no concept of files or directories — it only provides a linked
 * structure. The file/directory semantics come from the UnixFS layer embedded in the Data field.
 *
 * @see UnixFsDecoder
 */
object DagPbDecoder {
    fun decode(blockData: BlockData): DagPbNode {
        val pbNode = PBNode.parseFrom(blockData)

        val data = if (pbNode.hasData()) pbNode.data.toByteArray() else null

        val links = pbNode.linksList.map { pbLink ->
            require(pbLink.hasHash()) { "DAG-PB link is missing Hash field" }

            DagPbLink(
                hash = Cid.cast(pbLink.hash.toByteArray()),
                name = if (pbLink.hasName()) pbLink.name else null,
                size = if (pbLink.hasTsize()) pbLink.tsize else null
            )
        }

        return DagPbNode(data = data, links = links)
    }
}
