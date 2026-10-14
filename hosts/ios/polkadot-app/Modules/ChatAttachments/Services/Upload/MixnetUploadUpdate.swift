import Foundation
import Operation_iOS

struct MixnetUploadUpdate {
    let attachmentId: String
    let chunkHash: Data
    let uploadedSize: Int64
}

extension MixnetUploadUpdate: Operation_iOS.Identifiable {
    var identifier: String { attachmentId }
}
