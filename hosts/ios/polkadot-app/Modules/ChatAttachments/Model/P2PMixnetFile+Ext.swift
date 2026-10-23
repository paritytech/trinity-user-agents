import Foundation
import UniformTypeIdentifiers

extension ChatRemoteMessageContent.FileVariant {
    var identifier: Data {
        switch self {
        case let .p2pMixnet(file):
            file.identifier
        }
    }

    var claimTicket: Data {
        switch self {
        case let .p2pMixnet(file):
            file.claimTicket
        }
    }

    var meta: ChatRemoteMessageContent.FileMeta {
        switch self {
        case let .p2pMixnet(file):
            file.meta
        }
    }

    var node: ChatRemoteMessageContent.NodeEndpoint {
        switch self {
        case let .p2pMixnet(file):
            file.node
        }
    }

    var filename: String {
        let metadataHex = identifier.toHex()

        if let utType = UTType(mimeType: meta.mimeType),
           let fileExt = utType.preferredFilenameExtension {
            return (metadataHex as NSString).appendingPathExtension(fileExt) ?? metadataHex
        } else {
            return metadataHex
        }
    }
}
