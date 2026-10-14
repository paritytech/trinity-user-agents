import Foundation
import WebRTC

enum CallCreationState {
    case creating
    case ready(CallTracks)
    case closed(Error?)
}
