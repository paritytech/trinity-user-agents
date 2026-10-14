import Foundation
import Operation_iOS

struct MixnetUpload: Hashable {
    let attachmentId: String
    let ticket: Data?
    let node: String?
    let uploadedHashes: [Data]?
    let uploadedSize: Int64
}

extension MixnetUpload: Operation_iOS.Identifiable {
    var identifier: String { attachmentId }
}
