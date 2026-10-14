import Foundation
import Multicodec
import CID
import SubstrateSdk

public enum CIDCodec {
    case json
    case raw
    case directory
}

public protocol HexToCIDConverting: AnyObject {
    func convertToIPFSURL(fileHash: String, codec: CIDCodec) -> URL?
    func convertToIPFSURL(hash: Data, codec: CIDCodec) -> URL?
    /// For content already addressed by CID, such as a product manifest's icon.
    func ipfsURL(cid: String) -> URL
}

public final class HexToCIDConverter: HexToCIDConverting {
    private enum Constants {
        static let blake2b256Prefix = Data([0xA0, 0xE4, 0x02, 0x20])
    }

    private let ipfsBaseURL: URL

    public init(ipfsBaseURL: URL) {
        self.ipfsBaseURL = ipfsBaseURL
    }

    public func convertToIPFSURL(fileHash: String, codec: CIDCodec) -> URL? {
        guard let hashData = try? Data(hexString: fileHash) else {
            return nil
        }

        return convertToIPFSURL(hash: hashData, codec: codec)
    }

    public func convertToIPFSURL(hash: Data, codec: CIDCodec) -> URL? {
        guard let cid = convertToCID(hash: hash, codec: codec) else {
            return nil
        }
        return ipfsURL(cid: cid)
    }

    public func ipfsURL(cid: String) -> URL {
        ipfsBaseURL.appendingPathComponent(cid)
    }
}

private extension HexToCIDConverter {
    func convertToCID(hash: Data, codec: CIDCodec) -> String? {
        var cidBytes = Constants.blake2b256Prefix
        cidBytes.append(hash)
        let cid = try? CID(version: .v1, codec: codec.libValue, hash: Array(cidBytes))
        return cid?.toBaseEncodedString
    }
}

private extension CIDCodec {
    var libValue: Codecs {
        switch self {
        case .json:
            Codecs.json
        case .raw:
            Codecs.raw
        case .directory:
            Codecs.dag_pb
        }
    }
}
