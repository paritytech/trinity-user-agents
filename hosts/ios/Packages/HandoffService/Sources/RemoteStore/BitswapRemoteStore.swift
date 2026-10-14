import Foundation
import CID
import Multicodec
import NovaCrypto
import SDKLogger
import SubstrateSdk

/// Fetches promoted pool entries from on-chain storage via the node's `bitswap_v1_get`
/// JSON-RPC method (chat RFC 0001). The entry hash used for `hop_claim` is byte-for-byte
/// the CID digest, so any expired entry remains reachable under the hash the client holds.
public final class BitswapRemoteStore {
    let connection: JSONRPCEngine
    let logger: SDKLoggerProtocol?

    public init(connection: JSONRPCEngine, logger: SDKLoggerProtocol? = nil) {
        self.connection = connection
        self.logger = logger
    }
}

extension BitswapRemoteStore: LongTermRemoteStoring {
    public func downloadData(by fileHash: FileHash) async throws -> Data? {
        let cid = try makeCid(for: fileHash)

        do {
            let dataHex: String = try await connection.asyncCallMethod(
                RPCMethod.bitswapGet,
                params: [cid],
                options: JSONRPCOptions()
            )

            let data = try Data(hexString: dataHex)

            // The RPC path has no built-in integrity check: an entry failing
            // verification is treated as not found (RFC 0001).
            guard try data.blake2b32() == fileHash else {
                logger?.warning(
                    "Bitswap data failed integrity check for \(fileHash.toHex()) — treating as not found"
                )
                return nil
            }

            return data
        } catch let error as JSONRPCError where error.code == BitswapErrorCode.notFound {
            return nil
        } catch let error as JSONRPCError where error.code == BitswapErrorCode.invalidCid {
            // Spec: MUST NOT retry — a rejected CID means a client-side encoding bug.
            logger?.error("Node rejected CID \(cid) for \(fileHash.toHex())")
            throw error
        }
    }
}

private extension BitswapRemoteStore {
    enum Constants {
        // Multihash prefix: varint(0xb220 blake2b-256) + varint(32 digest length)
        static let blake2b256MultihashPrefix = Data([0xA0, 0xE4, 0x02, 0x20])
    }

    func makeCid(for fileHash: FileHash) throws -> String {
        var multihash = Constants.blake2b256MultihashPrefix
        multihash.append(fileHash)

        let cid = try CID(version: .v1, codec: .raw, hash: Array(multihash))

        return cid.toBaseEncodedString
    }
}
