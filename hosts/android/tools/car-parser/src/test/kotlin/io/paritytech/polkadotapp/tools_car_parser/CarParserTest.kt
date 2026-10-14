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
import java.io.ByteArrayOutputStream

class CarParserTest {
    @Test
    fun `parse single file archive`() {
        val fileContent = "Hello CAR!".toByteArray()

        val fileCid = createFakeCid("file")
        val rootCid = createFakeCid("root")

        val fileBlock = buildFileNode(fileContent)
        val rootBlock = buildDirectoryNode(links = listOf("index.html" to fileCid))

        val carBytes = buildCar(
            rootCid = rootCid,
            blocks = listOf(rootCid to rootBlock, fileCid to fileBlock)
        )

        val result = CarParser.parse(carBytes)

        assertTrue(result.isSuccess)
        val files = result.getOrThrow().files
        assertEquals(1, files.size)
        assertArrayEquals(fileContent, files["/index.html"])
    }

    @Test
    fun `parse multi-file archive`() {
        val htmlContent = "<html>hi</html>".toByteArray()
        val jsContent = "alert(1)".toByteArray()

        val htmlCid = createFakeCid("html")
        val jsCid = createFakeCid("js")
        val rootCid = createFakeCid("root")

        val rootBlock = buildDirectoryNode(
            links = listOf("index.html" to htmlCid, "app.js" to jsCid)
        )

        val carBytes = buildCar(
            rootCid = rootCid,
            blocks = listOf(
                rootCid to rootBlock,
                htmlCid to buildFileNode(htmlContent),
                jsCid to buildFileNode(jsContent)
            )
        )

        val result = CarParser.parse(carBytes)

        assertTrue(result.isSuccess)
        val files = result.getOrThrow().files
        assertEquals(2, files.size)
        assertArrayEquals(htmlContent, files["/index.html"])
        assertArrayEquals(jsContent, files["/app.js"])
    }

    @Test
    fun `parse nested directory archive`() {
        val cssContent = "body{}".toByteArray()

        val cssCid = createFakeCid("css")
        val stylesCid = createFakeCid("styles")
        val rootCid = createFakeCid("root")

        val carBytes = buildCar(
            rootCid = rootCid,
            blocks = listOf(
                rootCid to buildDirectoryNode(links = listOf("styles" to stylesCid)),
                stylesCid to buildDirectoryNode(links = listOf("main.css" to cssCid)),
                cssCid to buildFileNode(cssContent)
            )
        )

        val result = CarParser.parse(carBytes)

        assertTrue(result.isSuccess)
        val files = result.getOrThrow().files
        assertEquals(1, files.size)
        assertArrayEquals(cssContent, files["/styles/main.css"])
    }

    @Test
    fun `parse returns failure for truncated data`() {
        val rootCid = createFakeCid("root")
        val rootBlock = buildDirectoryNode(links = emptyList())
        val carBytes = buildCar(rootCid = rootCid, blocks = listOf(rootCid to rootBlock))

        // Truncate to break the format
        val truncated = carBytes.copyOfRange(0, carBytes.size / 2)

        val result = CarParser.parse(truncated)

        assertTrue(result.isFailure)
    }

    @Test
    fun `parse returns failure for empty input`() {
        val result = CarParser.parse(ByteArray(0))

        assertTrue(result.isFailure)
    }

    // -- CAR binary construction helpers --

    private fun buildCar(rootCid: Cid, blocks: List<Pair<Cid, BlockData>>): ByteArray {
        val headerCbor = buildCborHeader(rootCid)

        return buildByteArrayWithVarintPrefixes {
            // Header: varint length + CBOR bytes
            writeVarintPrefixed(headerCbor)

            // Blocks: varint length + (CID bytes + block data)
            for ((cid, data) in blocks) {
                val cidBytes = cid.toBytes()
                writeVarintPrefixed(cidBytes + data)
            }
        }
    }

    private fun buildCborHeader(rootCid: Cid): ByteArray {
        val cidWithPrefix = byteArrayOf(0x00) + rootCid.toBytes()

        // Manually build CBOR: {"version": 1, "roots": [<byte string>]}
        val out = ByteArrayOutputStream()
        out.write(0xA2) // map(2)
        // "version": 1
        out.write(0x67) // text(7)
        out.write("version".toByteArray())
        out.write(0x01) // uint(1)
        // "roots": [<cid bytes>]
        out.write(0x65) // text(5)
        out.write("roots".toByteArray())
        out.write(0x81.toInt()) // array(1)
        // CID as byte string (major type 2)
        if (cidWithPrefix.size < 24) {
            out.write(0x40 + cidWithPrefix.size)
        } else {
            out.write(0x58) // byte string, 1-byte length
            out.write(cidWithPrefix.size)
        }
        out.write(cidWithPrefix)

        return out.toByteArray()
    }

    private fun buildByteArrayWithVarintPrefixes(block: VarintOutputStream.() -> Unit): ByteArray {
        val out = ByteArrayOutputStream()
        VarintOutputStream(out).block()
        return out.toByteArray()
    }

    private class VarintOutputStream(private val out: ByteArrayOutputStream) {
        fun writeVarintPrefixed(data: ByteArray) {
            writeVarint(data.size)
            out.write(data)
        }

        private fun writeVarint(value: Int) {
            var v = value
            while (v > 0x7F) {
                out.write((v and 0x7F) or 0x80)
                v = v ushr 7
            }
            out.write(v and 0x7F)
        }
    }

    // -- Protobuf node construction helpers --

    private fun buildFileNode(content: ByteArray): BlockData {
        val unixFsData = UnixFsProto.Data.newBuilder()
            .setType(UnixFsProto.Data.DataType.File)
            .setData(ByteString.copyFrom(content))
            .setFilesize(content.size.toLong())
            .build()
            .toByteArray()

        return PBNode.newBuilder()
            .setData(ByteString.copyFrom(unixFsData))
            .build()
            .toByteArray()
    }

    private fun buildDirectoryNode(links: List<Pair<String, Cid>>): BlockData {
        val unixFsData = UnixFsProto.Data.newBuilder()
            .setType(UnixFsProto.Data.DataType.Directory)
            .build()
            .toByteArray()

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

    private fun createFakeCid(key: String): Cid {
        return Cid.buildCidV1(Cid.Codec.DagProtobuf, Multihash.Type.sha2_256, key.encodeToByteArray().padEnd(32))
    }
}
