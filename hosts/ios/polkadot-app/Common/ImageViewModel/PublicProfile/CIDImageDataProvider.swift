import Foundation
import Kingfisher
import TrUAPIHost

struct CIDImageDataProvider: ImageDataProvider {
    let reference: ImageReference
    let loadImage: @Sendable (ImageReference) async throws -> Data

    var cacheKey: String {
        switch reference {
        case let .bulletin(cid, _): cid
        }
    }

    func data() async throws -> Data {
        try Task.checkCancellation()
        let bytes = try await loadImage(reference)
        try Task.checkCancellation()
        return bytes
    }
}
