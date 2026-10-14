import Foundation
import WebRTC

struct CallTracks {
    let audioTrack: RTCAudioTrack?
    let videoTrack: RTCVideoTrack?

    init(audioTrack: RTCAudioTrack? = nil, videoTrack: RTCVideoTrack? = nil) {
        self.audioTrack = audioTrack
        self.videoTrack = videoTrack
    }

    func replacingFromRTPReceiver(_ rtpReceiver: RTCRtpReceiver) -> CallTracks {
        if let track = rtpReceiver.track as? RTCVideoTrack {
            return CallTracks(audioTrack: audioTrack, videoTrack: track)
        }

        if let track = rtpReceiver.track as? RTCAudioTrack {
            return CallTracks(audioTrack: track, videoTrack: videoTrack)
        }

        return self
    }
}
