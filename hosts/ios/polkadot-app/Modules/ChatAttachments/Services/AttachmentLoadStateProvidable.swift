import Foundation
import AsyncExtensions

enum AttachmentProgressEvent {
    struct Progress {
        let loaded: Int
        let total: Int
    }

    case onProgress(Progress)
    case onComplete
    case onFailure(Error)
}

protocol AttachmentLoadProgressProvidable {
    func subscribeState(for attachmentId: AttachmentId) async -> AnyAsyncSequence<AttachmentProgressEvent?>
}
