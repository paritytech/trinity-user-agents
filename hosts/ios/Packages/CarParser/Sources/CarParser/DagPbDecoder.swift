import CID
import Foundation
import SwiftProtobuf

enum DagPbDecoderError: Error {
    case missingLinkHash
}

struct DagPbLink {
    let cid: CID
    let name: String?
    let tsize: UInt64?
}

struct DagPbNode {
    let data: Data?
    let links: [DagPbLink]
}

enum DagPbDecoder {
    /// Decode a DAG-PB block from raw protobuf bytes.
    static func decode(from blockData: Data) throws -> DagPbNode {
        let pbNode = try PBNode(serializedBytes: blockData)

        let links: [DagPbLink] = try pbNode.links.map { pbLink in
            guard let hashBytes = pbLink.hash else {
                throw DagPbDecoderError.missingLinkHash
            }

            // Parse CID from the raw hash bytes in the link
            let parsedCid = try CidParser.parseCid(from: hashBytes, at: 0)

            return DagPbLink(
                cid: parsedCid.cid,
                name: pbLink.name,
                tsize: pbLink.tsize
            )
        }

        return DagPbNode(data: pbNode.data, links: links)
    }
}
