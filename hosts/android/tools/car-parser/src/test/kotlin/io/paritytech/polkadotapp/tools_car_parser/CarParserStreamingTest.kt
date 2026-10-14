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
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * Verifies the streaming [CarParser.unpack] reconstructs byte-for-byte the same files as the
 * buffered [CarParser.parse], across the codec/layout shapes the decoder handles.
 */
class CarParserStreamingTest {
    @Test
    fun `unpack matches parse for single file archive`() {
        val fileCid = createFakeCid("file")
        val rootCid = createFakeCid("root")

        assertParity(
            buildCar(
                rootCid = rootCid,
                blocks = listOf(
                    rootCid to buildDirectoryNode(links = listOf("index.html" to fileCid)),
                    fileCid to buildFileNode("Hello CAR!".toByteArray())
                )
            )
        )
    }

    @Test
    fun `unpack matches parse for multi-file archive`() {
        val htmlCid = createFakeCid("html")
        val jsCid = createFakeCid("js")
        val rootCid = createFakeCid("root")

        assertParity(
            buildCar(
                rootCid = rootCid,
                blocks = listOf(
                    rootCid to buildDirectoryNode(links = listOf("index.html" to htmlCid, "app.js" to jsCid)),
                    htmlCid to buildFileNode("<html>hi</html>".toByteArray()),
                    jsCid to buildFileNode("alert(1)".toByteArray())
                )
            )
        )
    }

    @Test
    fun `unpack matches parse for nested directory archive`() {
        val cssCid = createFakeCid("css")
        val stylesCid = createFakeCid("styles")
        val rootCid = createFakeCid("root")

        assertParity(
            buildCar(
                rootCid = rootCid,
                blocks = listOf(
                    rootCid to buildDirectoryNode(links = listOf("styles" to stylesCid)),
                    stylesCid to buildDirectoryNode(links = listOf("main.css" to cssCid)),
                    cssCid to buildFileNode("body{}".toByteArray())
                )
            )
        )
    }

    @Test
    fun `unpack matches parse for raw codec leaf file`() {
        val content = "raw file content".toByteArray()
        val rawFileCid = createRawCid("raw-file")
        val rootCid = createFakeCid("root")

        assertParity(
            buildCar(
                rootCid = rootCid,
                blocks = listOf(
                    rootCid to buildDirectoryNode(links = listOf("data.bin" to rawFileCid)),
                    rawFileCid to content
                )
            )
        )
    }

    @Test
    fun `unpack matches parse for chunked file with raw codec chunks`() {
        val chunk1 = "chunk-one-data".toByteArray()
        val chunk2 = "chunk-two-data".toByteArray()

        val rootCid = createFakeCid("root")
        val fileCid = createFakeCid("chunked-file")
        val chunk1Cid = createRawCid("chunk1")
        val chunk2Cid = createRawCid("chunk2")

        assertParity(
            buildCar(
                rootCid = rootCid,
                blocks = listOf(
                    rootCid to buildDirectoryNode(links = listOf("large.bin" to fileCid)),
                    fileCid to buildChunkedFileNode(
                        links = listOf(chunk1Cid, chunk2Cid),
                        blockSizes = listOf(chunk1.size.toLong(), chunk2.size.toLong())
                    ),
                    chunk1Cid to chunk1,
                    chunk2Cid to chunk2
                )
            )
        )
    }

    @Test
    fun `unpack throws for truncated data`() {
        val rootCid = createFakeCid("root")
        val carBytes = buildCar(rootCid = rootCid, blocks = listOf(rootCid to buildDirectoryNode(links = emptyList())))
        val truncated = carBytes.copyOfRange(0, carBytes.size / 2)

        assertThrows(Throwable::class.java) { unpackToMap(truncated) }
    }

    @Test
    fun `unpack throws for empty input`() {
        assertThrows(Throwable::class.java) { unpackToMap(ByteArray(0)) }
    }

    // -- Assertions --

    private fun assertParity(carBytes: ByteArray) {
        val buffered = CarParser.parse(carBytes).getOrThrow().files
        val streamed = unpackToMap(carBytes)

        assertEquals(buffered.keys, streamed.keys)
        buffered.forEach { (path, bytes) -> assertArrayEquals(bytes, streamed[path]) }
    }

    private fun unpackToMap(carBytes: ByteArray): Map<FilePath, FileContent> {
        val tempFile = File.createTempFile("test-car", ".car")
        return try {
            tempFile.writeBytes(carBytes)
            val files = mutableMapOf<FilePath, FileContent>()
            CarParser.unpack(tempFile) { path, content -> files[path] = content.readBytes() }
            files
        } finally {
            tempFile.delete()
        }
    }

    // -- CAR binary construction helpers --

    private fun buildCar(rootCid: Cid, blocks: List<Pair<Cid, BlockData>>): ByteArray {
        val headerCbor = buildCborHeader(rootCid)

        val out = ByteArrayOutputStream()
        val varint = VarintOutputStream(out)
        varint.writeVarintPrefixed(headerCbor)
        for ((cid, data) in blocks) {
            varint.writeVarintPrefixed(cid.toBytes() + data)
        }
        return out.toByteArray()
    }

    private fun buildCborHeader(rootCid: Cid): ByteArray {
        val cidWithPrefix = byteArrayOf(0x00) + rootCid.toBytes()

        val out = ByteArrayOutputStream()
        out.write(0xA2) // map(2)
        out.write(0x67) // text(7)
        out.write("version".toByteArray())
        out.write(0x01) // uint(1)
        out.write(0x65) // text(5)
        out.write("roots".toByteArray())
        out.write(0x81) // array(1)
        if (cidWithPrefix.size < 24) {
            out.write(0x40 + cidWithPrefix.size)
        } else {
            out.write(0x58) // byte string, 1-byte length
            out.write(cidWithPrefix.size)
        }
        out.write(cidWithPrefix)

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
            builder.addLinks(PBLink.newBuilder().setHash(ByteString.copyFrom(cid.toBytes())))
        }

        return builder.build().toByteArray()
    }

    private fun createFakeCid(key: String): Cid {
        return Cid.buildCidV1(Cid.Codec.DagProtobuf, Multihash.Type.sha2_256, key.encodeToByteArray().padEnd(32))
    }

    private fun createRawCid(key: String): Cid {
        return Cid.buildCidV1(Cid.Codec.Raw, Multihash.Type.sha2_256, key.encodeToByteArray().padEnd(32))
    }
}
