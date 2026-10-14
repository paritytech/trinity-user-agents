import Foundation
import Operation_iOS

enum MixnetDownloadEntryType: UInt8 {
    case metadata = 0
    case inline = 1
}

struct MixnetDownload: Hashable {
    let entryHashHex: String
    let entryType: MixnetDownloadEntryType
    let lastChunkIndex: Int32
    let totalChunks: Int32
    let metadata: Data?
    let downloadedBytes: Int64
}

extension MixnetDownload: Operation_iOS.Identifiable {
    var identifier: String { entryHashHex }
}

struct MixnetDownloadChunkIndex: Hashable {
    let entryHashHex: String
    let lastChunkIndex: Int32
    let downloadedBytes: Int64
}

extension MixnetDownloadChunkIndex: Operation_iOS.Identifiable {
    var identifier: String { entryHashHex }
}
