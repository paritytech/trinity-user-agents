package io.paritytech.polkadotapp.tools_car_parser

import com.google.protobuf.ByteString
import io.ipfs.multihash.Multihash
import io.paritytech.polkadotapp.common.utils.padEnd
import io.paritytech.polkadotapp.tools_car_parser.proto.MerkleDagProto.PBLink
import io.paritytech.polkadotapp.tools_car_parser.proto.MerkleDagProto.PBNode
import io.paritytech.polkadotapp.tools_car_parser.proto.UnixFsProto
import io.paritytech.polkadotapp.tools_ipfs_api.Cid
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class UnixFsDecoderTest {
    @Test
    fun `reconstructFileTree single file at root`() {
        val fileContent = "Hello World!".toByteArray()

        val rootCid = createFakeCid("root-cid")
        val fileCid = createFakeCid("file-cid")
        val blocks = mapOf(
            rootCid to buildDirectoryNode(links = listOf("index.html" to fileCid)),
            fileCid to buildFileNode(fileContent)
        )

        val result = UnixFsDecoder.reconstructFileTree(rootCid, blocks)

        assertEquals(1, result.size)
        assertTrue(result.containsKey("/index.html"))
        assertArrayEquals(fileContent, result["/index.html"])
    }

    @Test
    fun `reconstructFileTree flat directory with multiple files`() {
        val htmlContent = "<html>test</html>".toByteArray()
        val jsContent = "console.log('hi')".toByteArray()
        val cssContent = "body { }".toByteArray()

        val rootCid = createFakeCid("root-cid")
        val htmlCid = createFakeCid("html-cid")
        val jsCid = createFakeCid("js-cid")
        val cssCid = createFakeCid("css-cid")
        val blocks = mapOf(
            rootCid to buildDirectoryNode(
                links = listOf(
                    "index.html" to htmlCid,
                    "app.js" to jsCid,
                    "style.css" to cssCid
                )
            ),
            htmlCid to buildFileNode(htmlContent),
            jsCid to buildFileNode(jsContent),
            cssCid to buildFileNode(cssContent)
        )

        val result = UnixFsDecoder.reconstructFileTree(rootCid, blocks)

        assertEquals(3, result.size)
        assertArrayEquals(htmlContent, result["/index.html"])
        assertArrayEquals(jsContent, result["/app.js"])
        assertArrayEquals(cssContent, result["/style.css"])
    }

    @Test
    fun `reconstructFileTree nested directories`() {
        val logoContent = "PNG data".toByteArray()

        val rootCid = createFakeCid("root-cid")
        val assetsCid = createFakeCid("assets-cid")
        val logoCid = createFakeCid("logo-cid")
        val blocks = mapOf(
            rootCid to buildDirectoryNode(links = listOf("assets" to assetsCid)),
            assetsCid to buildDirectoryNode(links = listOf("logo.png" to logoCid)),
            logoCid to buildFileNode(logoContent)
        )

        val result = UnixFsDecoder.reconstructFileTree(rootCid, blocks)

        assertEquals(1, result.size)
        assertTrue(result.containsKey("/assets/logo.png"))
        assertArrayEquals(logoContent, result["/assets/logo.png"])
    }

    @Test
    fun `reconstructFileTree deeply nested directories with multiple files`() {
        val htmlContent = "<html>deep</html>".toByteArray()
        val cssContent = "body{}".toByteArray()
        val logoContent = "PNG".toByteArray()

        val rootCid = createFakeCid("root")
        val htmlCid = createFakeCid("html-cid")
        val assetsCid = createFakeCid("assets-cid")
        val cssDirCid = createFakeCid("css-dir-cid")
        val cssCid = createFakeCid("css-cid")
        val imagesDirCid = createFakeCid("images-dir-cid")
        val logoCid = createFakeCid("logo-cid")
        val blocks = mapOf(
            rootCid to buildDirectoryNode(
                links = listOf("index.html" to htmlCid, "assets" to assetsCid)
            ),
            htmlCid to buildFileNode(htmlContent),
            assetsCid to buildDirectoryNode(
                links = listOf("css" to cssDirCid, "images" to imagesDirCid)
            ),
            cssDirCid to buildDirectoryNode(links = listOf("main.css" to cssCid)),
            cssCid to buildFileNode(cssContent),
            imagesDirCid to buildDirectoryNode(links = listOf("logo.png" to logoCid)),
            logoCid to buildFileNode(logoContent)
        )

        val result = UnixFsDecoder.reconstructFileTree(rootCid, blocks)

        assertEquals(3, result.size)
        assertArrayEquals(htmlContent, result["/index.html"])
        assertArrayEquals(cssContent, result["/assets/css/main.css"])
        assertArrayEquals(logoContent, result["/assets/images/logo.png"])
    }

    @Test
    fun `reconstructFileTree empty directory`() {
        val rootCid = createFakeCid("root-cid")
        val blocks = mapOf(rootCid to buildDirectoryNode(links = emptyList()))

        val result = UnixFsDecoder.reconstructFileTree(rootCid, blocks)

        assertTrue(result.isEmpty())
    }

    @Test
    fun `reconstructFileTree single file no chunking`() {
        val content = "single file content".toByteArray()

        val rootCid = createFakeCid("root-cid")
        val fileCid = createFakeCid("file-cid")
        val blocks = mapOf(
            rootCid to buildDirectoryNode(links = listOf("readme.txt" to fileCid)),
            fileCid to buildFileNode(content)
        )

        val result = UnixFsDecoder.reconstructFileTree(rootCid, blocks)

        assertEquals(1, result.size)
        assertArrayEquals(content, result["/readme.txt"])
    }

    @Test
    fun `reconstructFileTree raw codec block as leaf file`() {
        val content = "raw file content".toByteArray()

        val rootCid = createFakeCid("root-cid")
        val rawFileCid = createRawCid("raw-file")
        val blocks = mapOf(
            rootCid to buildDirectoryNode(links = listOf("data.bin" to rawFileCid)),
            rawFileCid to content // Raw codec — data is the file content itself, not protobuf
        )

        val result = UnixFsDecoder.reconstructFileTree(rootCid, blocks)

        assertEquals(1, result.size)
        assertArrayEquals(content, result["/data.bin"])
    }

    @Test
    fun `reconstructFileTree chunked file with raw codec chunks`() {
        val chunk1 = "chunk-one-data".toByteArray()
        val chunk2 = "chunk-two-data".toByteArray()

        val rootCid = createFakeCid("root")
        val fileCid = createFakeCid("chunked-file")
        val chunk1Cid = createRawCid("chunk1")
        val chunk2Cid = createRawCid("chunk2")

        val chunkedFileNode = buildChunkedFileNode(
            links = listOf(chunk1Cid, chunk2Cid),
            blockSizes = listOf(chunk1.size.toLong(), chunk2.size.toLong())
        )

        val blocks = mapOf(
            rootCid to buildDirectoryNode(links = listOf("large.bin" to fileCid)),
            fileCid to chunkedFileNode,
            chunk1Cid to chunk1,
            chunk2Cid to chunk2
        )

        val result = UnixFsDecoder.reconstructFileTree(rootCid, blocks)

        assertEquals(1, result.size)
        assertArrayEquals(chunk1 + chunk2, result["/large.bin"])
    }

    // -- Helpers using protobuf builders --

    private fun buildUnixFs(
        type: UnixFsProto.Data.DataType,
        data: ByteArray? = null,
        fileSize: Long? = null,
        blockSizes: List<Long> = emptyList()
    ): BlockData {
        val builder = UnixFsProto.Data.newBuilder().setType(type)
        if (data != null) builder.setData(ByteString.copyFrom(data))
        if (fileSize != null) builder.setFilesize(fileSize)
        blockSizes.forEach { builder.addBlocksizes(it) }
        return builder.build().toByteArray()
    }

    private fun buildFileNode(content: ByteArray): BlockData {
        val unixFsData = buildUnixFs(
            type = UnixFsProto.Data.DataType.File,
            data = content,
            fileSize = content.size.toLong()
        )
        return PBNode.newBuilder()
            .setData(ByteString.copyFrom(unixFsData))
            .build()
            .toByteArray()
    }

    private fun buildDirectoryNode(links: List<Pair<String, Cid>>): BlockData {
        val unixFsData = buildUnixFs(type = UnixFsProto.Data.DataType.Directory)
        val builder = PBNode.newBuilder()
            .setData(ByteString.copyFrom(unixFsData))

        for ((name, cid) in links) {
            builder.addLinks(
                PBLink.newBuilder()
                    .setHash(ByteString.copyFrom(cid.toBytes()))
                    .setName(name)
            )
        }

        return builder.build().toByteArray()
    }

    private fun buildChunkedFileNode(links: List<Cid>, blockSizes: List<Long>): BlockData {
        val unixFsData = buildUnixFs(
            type = UnixFsProto.Data.DataType.File,
            blockSizes = blockSizes
        )
        val builder = PBNode.newBuilder()
            .setData(ByteString.copyFrom(unixFsData))

        for (cid in links) {
            builder.addLinks(
                PBLink.newBuilder()
                    .setHash(ByteString.copyFrom(cid.toBytes()))
            )
        }

        return builder.build().toByteArray()
    }

    private fun createFakeCid(key: String): Cid {
        return Cid(0, Cid.Codec.DagProtobuf, Multihash.Type.sha2_256, key.encodeToByteArray().padEnd(32))
    }

    private fun createRawCid(key: String): Cid {
        return Cid.buildCidV1(Cid.Codec.Raw, Multihash.Type.sha2_256, key.encodeToByteArray().padEnd(32))
    }
}
