import CID
import Foundation
import Multicodec
import Testing
import VarInt
@testable import CarParser
import SwiftProtobuf

// MARK: - CID Parser Tests

@Suite("CidParser")
struct CidParserTests {
    @Test func parseCidV0() throws {
        var cidBytes = Data([0x12, 0x20])
        cidBytes.append(Data(repeating: 0xAB, count: 32))

        let parsed = try CidParser.parseCid(from: cidBytes, at: 0)
        #expect(parsed.totalBytesRead == 34)
        #expect(parsed.cid.codec == .dag_pb)
        #expect(parsed.cid.rawData.count == 36) // CID reconstructs with version + codec prefix
    }

    @Test func parseCidV1Raw() throws {
        var cidBytes = Data([0x01, 0x55, 0x12, 0x20])
        cidBytes.append(Data(repeating: 0xCD, count: 32))

        let parsed = try CidParser.parseCid(from: cidBytes, at: 0)
        #expect(parsed.totalBytesRead == 36)
        #expect(parsed.cid.codec == .raw)
    }

    @Test func parseCidV1DagPb() throws {
        var cidBytes = Data([0x01, 0x70, 0x12, 0x20])
        cidBytes.append(Data(repeating: 0xEF, count: 32))

        let parsed = try CidParser.parseCid(from: cidBytes, at: 0)
        #expect(parsed.totalBytesRead == 36)
        #expect(parsed.cid.codec == .dag_pb)
    }
}

// MARK: - CAR Parser Integration Tests

@Suite("CarParser Integration")
struct CarParserIntegrationTests {
    // MARK: - Test helpers

    private func makeCidV1(codec: Codecs, hashBytes: Data) -> Data {
        var cid = Data()
        cid.append(0x01)
        cid.append(contentsOf: putUVarInt(codec.code))
        cid.append(0x12) // sha2-256
        cid.append(UInt8(hashBytes.count))
        cid.append(hashBytes)
        return cid
    }

    private func buildCborHeader(rootCid: Data) -> Data {
        var header = Data()
        header.append(0xA2) // map(2)

        header.append(0x67) // text(7)
        header.append(contentsOf: "version".utf8)
        header.append(0x01) // uint(1)

        header.append(0x65) // text(5)
        header.append(contentsOf: "roots".utf8)
        header.append(0x81) // array(1)

        let cidWithPrefix = Data([0x00]) + rootCid
        if cidWithPrefix.count <= 23 {
            header.append(0x40 + UInt8(cidWithPrefix.count))
        } else {
            header.append(0x58)
            header.append(UInt8(cidWithPrefix.count))
        }
        header.append(cidWithPrefix)

        return header
    }

    private func buildCar(rootCid: Data, blocks: [(cid: Data, data: Data)]) -> Data {
        let header = buildCborHeader(rootCid: rootCid)
        var car = Data()
        car.append(contentsOf: putUVarInt(UInt64(header.count)))
        car.append(header)

        for block in blocks {
            let blockContent = block.cid + block.data
            car.append(contentsOf: putUVarInt(UInt64(blockContent.count)))
            car.append(blockContent)
        }
        return car
    }

    private func buildDirectoryNode(links: [(name: String, cid: Data)]) -> Data {
        var unixFs = UnixFsData()
        unixFs.type = .directory

        var pbNode = PBNode()
        pbNode.data = try! unixFs.serializedData()
        pbNode.links = links.map { link in
            var pbLink = PBLink()
            pbLink.hash = link.cid
            pbLink.name = link.name
            return pbLink
        }
        return try! pbNode.serializedData()
    }

    private func buildFileLeafNode(content: Data) -> Data {
        var unixFs = UnixFsData()
        unixFs.type = .file
        unixFs.data = content

        var pbNode = PBNode()
        pbNode.data = try! unixFs.serializedData()
        return try! pbNode.serializedData()
    }

    private func buildChunkedFileNode(linkCids: [Data], blockSizes: [UInt64]) -> Data {
        var unixFs = UnixFsData()
        unixFs.type = .file
        unixFs.blocksizes = blockSizes

        var pbNode = PBNode()
        pbNode.data = try! unixFs.serializedData()
        pbNode.links = linkCids.map { cid in
            var pbLink = PBLink()
            pbLink.hash = cid
            return pbLink
        }
        return try! pbNode.serializedData()
    }

    // MARK: - Tests

    private func unpackToMap(data: Data) throws -> [String: Data] {
        var map: [String: Data] = [:]
        try CarParser.unpack(data: data) { path, produce in
            var content = Data()
            try produce { window in content.append(window) }
            map[path] = content
        }
        return map
    }

    private func unpackToMap(fileURL: URL) throws -> [String: Data] {
        var map: [String: Data] = [:]
        try CarParser.unpack(fileURL: fileURL) { path, produce in
            var content = Data()
            try produce { window in content.append(window) }
            map[path] = content
        }
        return map
    }

    @Test func singleFileArchive() throws {
        let fileContent = Data("Hello, IPFS!".utf8)
        let fileCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x01, count: 32))
        let fileNode = buildFileLeafNode(content: fileContent)

        let rootCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x02, count: 32))
        let rootNode = buildDirectoryNode(links: [("hello.txt", fileCid)])

        let car = buildCar(rootCid: rootCid, blocks: [
            (rootCid, rootNode),
            (fileCid, fileNode),
        ])

        let files = try unpackToMap(data: car)
        #expect(files.count == 1)
        #expect(files["hello.txt"] == fileContent)
    }

    @Test func multiFileArchive() throws {
        let content1 = Data("file1".utf8)
        let content2 = Data("file2".utf8)

        let cid1 = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x01, count: 32))
        let cid2 = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x02, count: 32))
        let rootCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x03, count: 32))

        let car = buildCar(rootCid: rootCid, blocks: [
            (rootCid, buildDirectoryNode(links: [("a.txt", cid1), ("b.txt", cid2)])),
            (cid1, buildFileLeafNode(content: content1)),
            (cid2, buildFileLeafNode(content: content2)),
        ])

        let files = try unpackToMap(data: car)
        #expect(files.count == 2)
        #expect(files["a.txt"] == content1)
        #expect(files["b.txt"] == content2)
    }

    @Test func nestedDirectoryArchive() throws {
        let content = Data("nested file".utf8)
        let fileCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x01, count: 32))
        let subdirCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x02, count: 32))
        let rootCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x03, count: 32))

        let car = buildCar(rootCid: rootCid, blocks: [
            (rootCid, buildDirectoryNode(links: [("subdir", subdirCid)])),
            (subdirCid, buildDirectoryNode(links: [("deep.txt", fileCid)])),
            (fileCid, buildFileLeafNode(content: content)),
        ])

        let files = try unpackToMap(data: car)
        #expect(files.count == 1)
        #expect(files["subdir/deep.txt"] == content)
    }

    @Test func rawCodecLeaf() throws {
        let content = Data("raw leaf".utf8)
        let rawCid = makeCidV1(codec: .raw, hashBytes: Data(repeating: 0x01, count: 32))
        let rootCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x02, count: 32))

        let car = buildCar(rootCid: rootCid, blocks: [
            (rootCid, buildDirectoryNode(links: [("raw.bin", rawCid)])),
            (rawCid, content),
        ])

        let files = try unpackToMap(data: car)
        #expect(files.count == 1)
        #expect(files["raw.bin"] == content)
    }

    @Test func chunkedFile() throws {
        let chunk1 = Data("chunk1".utf8)
        let chunk2 = Data("chunk2".utf8)

        let chunkCid1 = makeCidV1(codec: .raw, hashBytes: Data(repeating: 0x01, count: 32))
        let chunkCid2 = makeCidV1(codec: .raw, hashBytes: Data(repeating: 0x02, count: 32))
        let fileCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x03, count: 32))
        let rootCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x04, count: 32))

        let car = buildCar(rootCid: rootCid, blocks: [
            (rootCid, buildDirectoryNode(links: [("big.txt", fileCid)])),
            (fileCid, buildChunkedFileNode(
                linkCids: [chunkCid1, chunkCid2],
                blockSizes: [UInt64(chunk1.count), UInt64(chunk2.count)]
            )),
            (chunkCid1, chunk1),
            (chunkCid2, chunk2),
        ])

        let files = try unpackToMap(data: car)
        #expect(files.count == 1)
        #expect(files["big.txt"] == chunk1 + chunk2)
    }

    @Test func emptyInputFails() {
        #expect(throws: (any Error).self) {
            try CarParser.unpack(data: Data()) { _, _ in }
        }
    }

    @Test func truncatedInputFails() {
        let truncated = Data([0x50, 0xA2])
        #expect(throws: (any Error).self) {
            try CarParser.unpack(data: truncated) { _, _ in }
        }
    }

    @Test func looksLikeCarArchive() throws {
        let fileCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x01, count: 32))
        let rootCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x02, count: 32))

        let car = buildCar(rootCid: rootCid, blocks: [
            (rootCid, buildDirectoryNode(links: [("f.txt", fileCid)])),
            (fileCid, buildFileLeafNode(content: Data("test".utf8))),
        ])

        #expect(CarParser.looksLikeCarArchive(car))
        #expect(!CarParser.looksLikeCarArchive(Data()))
        #expect(!CarParser.looksLikeCarArchive(Data("not a car file".utf8)))
    }

    @Test func unpackFromFileProducesExpected() throws {
        let content1 = Data("file1".utf8)
        let content2 = Data("file2".utf8)

        let cid1 = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x01, count: 32))
        let cid2 = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x02, count: 32))
        let rootCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x03, count: 32))

        let car = buildCar(rootCid: rootCid, blocks: [
            (rootCid, buildDirectoryNode(links: [("a.txt", cid1), ("b.txt", cid2)])),
            (cid1, buildFileLeafNode(content: content1)),
            (cid2, buildFileLeafNode(content: content2)),
        ])

        let tempURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("test_car_\(UUID().uuidString).car")
        defer { try? FileManager.default.removeItem(at: tempURL) }

        try car.write(to: tempURL)

        let unpackedFromFile = try unpackToMap(fileURL: tempURL)
        #expect(unpackedFromFile == ["a.txt": content1, "b.txt": content2])
    }

    @Test func chunkedFileReassemblesToByteIdenticalThroughWindows() throws {
        let chunk1 = Data("chunk1".utf8)
        let chunk2 = Data("chunk2".utf8)

        let chunkCid1 = makeCidV1(codec: .raw, hashBytes: Data(repeating: 0x01, count: 32))
        let chunkCid2 = makeCidV1(codec: .raw, hashBytes: Data(repeating: 0x02, count: 32))
        let fileCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x03, count: 32))
        let rootCid = makeCidV1(codec: .dag_pb, hashBytes: Data(repeating: 0x04, count: 32))

        let car = buildCar(rootCid: rootCid, blocks: [
            (rootCid, buildDirectoryNode(links: [("big.txt", fileCid)])),
            (fileCid, buildChunkedFileNode(
                linkCids: [chunkCid1, chunkCid2],
                blockSizes: [UInt64(chunk1.count), UInt64(chunk2.count)]
            )),
            (chunkCid1, chunk1),
            (chunkCid2, chunk2),
        ])

        let expected = chunk1 + chunk2

        var unpackedFile: [String: Data] = [:]
        try CarParser.unpack(data: car) { path, produce in
            var data = Data()
            try produce { window in
                data.append(window)
            }
            unpackedFile[path] = data
        }

        #expect(unpackedFile["big.txt"] == expected)
    }
}
