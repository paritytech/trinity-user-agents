import Foundation
import WebRTC

typealias VideoAttachmentClosure = (RTCVideoRenderer) -> Void

struct ChatCallRendererModel {
    let attach: VideoAttachmentClosure?

    var hasVideo: Bool {
        attach != nil
    }
}
