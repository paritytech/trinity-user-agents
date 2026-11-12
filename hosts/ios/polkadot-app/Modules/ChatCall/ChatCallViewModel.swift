import Observation
import PolkadotUI
import SwiftUI
import WebRTC

protocol ChatCallViewModelProtocol {
    var username: String { get set }
    var avatarViewModel: AvatarViewModel { get set }
    var callType: ChatCallType { get set }
    var callState: ChatCallState { get set }
    var connectedAt: Date? { get set }
    var remoteRenderingModel: ChatCallRendererModel? { get set }
    var localRenderingModel: ChatCallRendererModel? { get set }
    var isIncoming: Bool { get set }
    var isMuted: Bool { get set }
    var isVideoEnabled: Bool { get set }
    var remoteMediaState: CallRemoteMediaState { get set }
    var onAcceptCall: (() -> Void)? { get set }
    var onEndCall: (() -> Void)? { get set }
    var onToggleMute: (() -> Void)? { get set }
    var onToggleVideo: (() -> Void)? { get set }
    var onSelectAudioRoute: ((CallAudioRoute) -> Void)? { get set }
}

@Observable
class ChatCallViewModel: ChatCallViewModelProtocol {
    var username: String = ""
    var avatarViewModel: AvatarViewModel = .colored(text: "", colorSeed: "")
    var callType: ChatCallType = .audio
    var callState: ChatCallState = .ringing
    var connectedAt: Date?
    var remoteRenderingModel: ChatCallRendererModel?
    var localRenderingModel: ChatCallRendererModel?
    var isIncoming: Bool = false
    var isMuted: Bool = false
    var isVideoEnabled: Bool = false
    var remoteMediaState = CallRemoteMediaState(isCameraEnabled: false, isMicrophoneEnabled: true)
    var capability: ChatCallCapability = .none
    var audioRouteState: CallAudioRouteState = .unknown
    var onAcceptCall: (() -> Void)?
    var onEndCall: (() -> Void)?
    var onToggleMute: (() -> Void)?
    var onToggleVideo: (() -> Void)?
    var onSelectAudioRoute: ((CallAudioRoute) -> Void)?

    var onCall: Bool {
        if isIncoming {
            callState == .connected || callState == .connecting
        } else {
            callState != .ended && callState != .failed
        }
    }

    var canEndCall: Bool {
        callState != .ended && callState != .failed
    }

    var shouldDisplayMute: Bool {
        capability.contains(.mute) && onCall
    }

    var shouldDisplayVideoToggle: Bool {
        capability.contains(.video) && onCall
    }

    var shouldShowRemoteVideo: Bool {
        remoteRenderingModel?.hasVideo == true && remoteMediaState.isCameraEnabled
    }

    var shouldShowSelfMicOffBanner: Bool {
        callState == .connected && isMuted
    }

    var shouldShowRemoteMicOffBanner: Bool {
        callState == .connected && !remoteMediaState.isMicrophoneEnabled
    }

    var shouldDisplayAudioRoute: Bool {
        capability.contains(.audioRoute) && onCall
    }
}
