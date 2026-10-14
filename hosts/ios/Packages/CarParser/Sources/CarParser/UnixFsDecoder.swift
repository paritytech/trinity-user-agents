import CID
import Foundation
import Multicodec

enum UnixFsDecoderError: Error {
    case blockNotFound(CID)
    case invalidNode(String)
    case missingLinkName
}

/// Represents decoded UnixFS content types.
enum UnixFsContent {
    case directory
    case leaf(Data)
    case chunkedFile
}

/// Block reference: index into source data and codec.
struct BlockRef {
    let range: Range<Int>
    let codec: Codecs
}

enum UnixFsDecoder {
    /// Reconstruct the file tree but emit each file to `emit` one at a time instead of
    /// accumulating a map, so the full unpacked archive is never resident at once.
    static func streamFileTree(
        rootCid: CID,
        data: Data,
        blocks: [CID: BlockRef],
        emit: (_ path: String, _ produce: (BlockWindowSink) throws -> Void) throws -> Void
    ) throws {
        try traverseNode(cid: rootCid, currentPath: "", data: data, blocks: blocks, emit: emit)
    }

    // MARK: - Private

    private static func traverseNode(
        cid: CID,
        currentPath: String,
        data: Data,
        blocks: [CID: BlockRef],
        emit: (_ path: String, _ produce: (BlockWindowSink) throws -> Void) throws -> Void
    ) throws {
        guard let blockRef = blocks[cid] else {
            throw UnixFsDecoderError.blockNotFound(cid)
        }

        // Raw codec blocks are leaf file content
        if blockRef.codec == .raw {
            try emit(currentPath) { sink in
                try drainBlockInWindows(data[blockRef.range], sink: sink)
            }
            return
        }

        // DAG-PB block: decode protobuf
        let dagPbNode = try DagPbDecoder.decode(from: Data(data[blockRef.range]))

        // Decode UnixFS data if present
        let unixFs: UnixFsContent? = try dagPbNode.data.map { try decodeUnixFs($0) }

        switch unixFs {
        case .directory,
             .none:
            // Directory or node with links but no UnixFS: recurse into children
            for link in dagPbNode.links {
                guard let name = link.name else {
                    throw UnixFsDecoderError.missingLinkName
                }
                let childPath = currentPath.isEmpty ? name : "\(currentPath)/\(name)"
                try traverseNode(cid: link.cid, currentPath: childPath, data: data, blocks: blocks, emit: emit)
            }

        case let .leaf(content):
            try emit(currentPath) { sink in
                try drainBlockInWindows(content, sink: sink)
            }

        case .chunkedFile:
            try emit(currentPath) { sink in
                try assembleChunkedFile(dagPbNode: dagPbNode, data: data, blocks: blocks, sink: sink)
            }
        }
    }

    private static func decodeUnixFs(_ data: Data) throws -> UnixFsContent {
        let proto = try UnixFsData(serializedBytes: data)

        switch proto.type {
        case .directory:
            return .directory

        case .raw:
            return .leaf(proto.data ?? Data())

        case .file:
            if !proto.blocksizes.isEmpty {
                return .chunkedFile
            }
            return .leaf(proto.data ?? Data())

        default:
            return .leaf(proto.data ?? Data())
        }
    }

    private static func assembleChunkedFile(
        dagPbNode: DagPbNode,
        data: Data,
        blocks: [CID: BlockRef],
        sink: BlockWindowSink
    ) throws {
        for link in dagPbNode.links {
            guard let blockRef = blocks[link.cid] else {
                throw UnixFsDecoderError.blockNotFound(link.cid)
            }

            if blockRef.codec == .raw {
                // Raw codec: drain the referenced data in windows
                try drainBlockInWindows(data[blockRef.range], sink: sink)
            } else {
                // DAG-PB: may be a nested chunk or a leaf
                let childNode = try DagPbDecoder.decode(from: Data(data[blockRef.range]))

                if !childNode.links.isEmpty {
                    // Nested chunks
                    try assembleChunkedFile(dagPbNode: childNode, data: data, blocks: blocks, sink: sink)
                } else if let protoData = childNode.data {
                    let unixFs = try UnixFsData(serializedBytes: protoData)
                    if let content = unixFs.data {
                        try drainBlockInWindows(content, sink: sink)
                    }
                }
            }
        }
    }

    private static func drainBlockInWindows(_ data: Data, sink: BlockWindowSink) throws {
        let windowSize = 1 << 20 // 1MB
        var lower = data.startIndex
        while lower < data.endIndex {
            let upper = data.index(lower, offsetBy: windowSize, limitedBy: data.endIndex) ?? data.endIndex
            try sink(Data(data[lower ..< upper]))
            lower = upper
        }
    }
}
