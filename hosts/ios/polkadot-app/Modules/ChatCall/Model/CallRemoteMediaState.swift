import Foundation

struct CallRemoteMediaState: Equatable {
    var isCameraEnabled: Bool
    var isMicrophoneEnabled: Bool

    func applying(_ signal: CallMediaStateSignal) -> CallRemoteMediaState {
        var state = self

        switch signal {
        case let .cameraEnabled(isEnabled):
            state.isCameraEnabled = isEnabled
        case let .microphoneEnabled(isEnabled):
            state.isMicrophoneEnabled = isEnabled
        }

        return state
    }
}
