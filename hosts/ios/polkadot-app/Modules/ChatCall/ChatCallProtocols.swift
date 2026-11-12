import UIKit
import PolkadotUI
import WebRTC
import UIKitExt

protocol ChatCallViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: ChatCallViewLayout.ViewModel)
    func didUpdateCallState(_ state: ChatCallState)
    func didUpdateConnectedAt(_ date: Date?)
    func didReceiveRemoteRenderer(model: ChatCallRendererModel)
    func didReceiveLocalRenderer(model: ChatCallRendererModel)
    func didUpdateAudioRoute(_ state: CallAudioRouteState)
    func didUpdateMuteState(_ muted: Bool)
    func didUpdateVideoState(_ isEnabled: Bool)
    func didReceiveCapability(_ capability: ChatCallCapability)
    func didUpdateRemoteMediaState(_ state: CallRemoteMediaState)
}

@MainActor
protocol ChatCallPresenterProtocol: AnyObject {
    func setup()
    func acceptCall()
    func endCall()
    func toggleMute()
    func toggleVideo()
    func selectAudioRoute(_ route: CallAudioRoute)
}

protocol ChatCallInteractorInputProtocol: AnyObject {
    func setup()
    func acceptCall()
    func endCall()
    func toggleMute()
    func toggleVideo()
    func selectAudioRoute(_ route: CallAudioRoute)
}

@MainActor
protocol ChatCallInteractorOutputProtocol: AnyObject {
    func didUpdateCallState(_ state: ChatCallState)
    func didRequireMicrophoneAccess()
    func didRequireCameraAccess()
    func didFailVideoCapture()
    func didUpdateConnectedAt(_ date: Date?)
    func didEndCall()
    func didReceiveRemoteRenderer(model: ChatCallRendererModel)
    func didReceiveLocalRenderer(model: ChatCallRendererModel)
    func didUpdateAudioRoute(_ state: CallAudioRouteState)
    func didUpdateMuteState(_ muted: Bool)
    func didUpdateVideoState(_ isEnabled: Bool)
    func didReceiveCapability(_ capability: ChatCallCapability)
    func didUpdateRemoteMediaState(_ state: CallRemoteMediaState)
}

@MainActor
protocol ChatCallWireframeProtocol: AnyObject {
    func close(from view: ChatCallViewProtocol?)
    func presentMicrophoneAccessRequired(from view: ChatCallViewProtocol?)
    func presentCameraAccessRequired(from view: ChatCallViewProtocol?)
    func presentVideoCaptureFailed(from view: ChatCallViewProtocol?)
}

enum ChatCallState {
    case contacting
    case ringing
    case connecting
    case connected
    case ended
    case failed
}
