import CID
import Foundation

public enum CarParserError: Error {
    case dataTooSmall
    case invalidHeader(Error)
    case blockParseFailed(Error)
    case fileTreeReconstructionFailed(Error)
}

public typealias BlockWindowSink = (_ window: Data) throws -> Void
public typealias CarContentWriter = (_ path: String, _ produce: (BlockWindowSink) throws -> Void) throws -> Void

public enum CarParser {
    /// Streaming unpack from a CAR file on disk. The file is memory-mapped rather than read into a
    /// heap buffer, so the archive bytes are paged by the OS instead of held resident.
    ///
    /// Mapping (not a sequential FileHandle) is deliberate: DAG traversal follows block links by CID
    /// in arbitrary order, so reads are random-access. A memory map serves scattered reads as O(1)
    /// views without copying; a FileHandle would force a seek+read (and heap copy) per block, or
    /// buffering the whole archive. Note `.mappedIfSafe` is only a hint — if the OS declines to map,
    /// it falls back to a full read, but the block-index design avoids per-block copies regardless.
    public static func unpack(
        fileURL: URL,
        writer: CarContentWriter
    ) throws {
        let data = try Data(contentsOf: fileURL, options: .mappedIfSafe)
        try unpack(data: data, writer: writer)
    }

    /// Heuristic check: does this data look like a CARv1 archive?
    /// Checks for valid varint + CBOR header with version 1.
    public static func looksLikeCarArchive(_ data: Data) -> Bool {
        guard data.count >= 10 else { return false }

        do {
            let (headerLen, headerVarIntSize) = try data.readUVarInt(at: 0)
            let headerEnd = headerVarIntSize + Int(headerLen)

            guard headerEnd <= data.count else { return false }

            let headerBytes = data[data.startIndex + headerVarIntSize ..< data.startIndex + headerEnd]
            let header = try CarHeaderDecoder.decode(from: Data(headerBytes))

            return header.version == 1 && !header.roots.isEmpty
        } catch {
            return false
        }
    }
}

extension CarParser {
    /// Streaming unpack: reconstructs the archive and emits each file to `writer` one at a time,
    /// without accumulating the full file map in memory.
    static func unpack(
        data: Data,
        writer: CarContentWriter
    ) throws {
        let (header, blocks) = try parseHeaderAndBlocks(data: data)
        let rootCid = try rootCid(from: header)

        do {
            try UnixFsDecoder.streamFileTree(rootCid: rootCid, data: data, blocks: blocks, emit: writer)
        } catch let error as CarParserError {
            throw error
        } catch let error as UnixFsDecoderError {
            throw CarParserError.fileTreeReconstructionFailed(error)
        }
    }

    static func parseHeaderAndBlocks(data: Data) throws -> (CarHeader, [CID: BlockRef]) {
        guard data.count >= 10 else {
            throw CarParserError.dataTooSmall
        }

        var offset = 0

        let (headerLen, headerVarIntSize) = try data.readUVarInt(at: offset)
        offset += headerVarIntSize

        let headerEnd = offset + Int(headerLen)
        guard headerEnd <= data.count else {
            throw CarParserError.dataTooSmall
        }

        let headerBytes = data[data.startIndex + offset ..< data.startIndex + headerEnd]
        let header: CarHeader
        do {
            header = try CarHeaderDecoder.decode(from: Data(headerBytes))
        } catch {
            throw CarParserError.invalidHeader(error)
        }

        offset = headerEnd

        var blocks: [CID: BlockRef] = [:]

        while offset < data.count {
            let (blockLen, blockVarIntSize) = try data.readUVarInt(at: offset)
            offset += blockVarIntSize

            let blockEnd = offset + Int(blockLen)
            guard blockEnd <= data.count else {
                throw CarParserError.dataTooSmall
            }

            let parsedCid: ParsedCid
            do {
                parsedCid = try CidParser.parseCid(from: data, at: offset)
            } catch {
                throw CarParserError.blockParseFailed(error)
            }

            let dataStart = offset + parsedCid.totalBytesRead
            let blockRange = (data.startIndex + dataStart) ..< (data.startIndex + blockEnd)

            blocks[parsedCid.cid] = BlockRef(range: blockRange, codec: parsedCid.cid.codec)

            offset = blockEnd
        }

        return (header, blocks)
    }

    static func rootCid(from header: CarHeader) throws -> CID {
        guard let rootCidBytes = header.roots.first else {
            throw CarParserError.invalidHeader(CarHeaderError.noRoots)
        }

        do {
            return try CidParser.parseCid(from: rootCidBytes, at: 0).cid
        } catch {
            throw CarParserError.blockParseFailed(error)
        }
    }
}
