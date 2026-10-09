import Foundation
import PolkadotUI
import WebRTC

final class ChatCallPresenter {
    weak var view: ChatCallViewProtocol?
    let wireframe: ChatCallWireframeProtocol
    let interactor: ChatCallInteractorInputProtocol
    let peer: CallPeer
    let role: CallRole
    let callType: ChatCallType

    init(
        peer: CallPeer,
        role: CallRole,
        callType: ChatCallType,
        interactor: ChatCallInteractorInputProtocol,
        wireframe: ChatCallWireframeProtocol
    ) {
        self.peer = peer
        self.role = role
        self.callType = callType
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension ChatCallPresenter: ChatCallPresenterProtocol {
    func setup() {
        provideViewModel()
        interactor.setup()
    }

    func acceptCall() {
        interactor.acceptCall()
    }

    func endCall() {
        interactor.endCall()
    }

    func toggleMute() {
        interactor.toggleMute()
    }

    func toggleVideo() {
        interactor.toggleVideo()
    }

    func selectAudioRoute(_ route: CallAudioRoute) {
        interactor.selectAudioRoute(route)
    }
}

extension ChatCallPresenter: ChatCallInteractorOutputProtocol {
    func didUpdateCallState(_ state: ChatCallState) {
        view?.didUpdateCallState(state)
    }

    func didRequireMicrophoneAccess() {
        wireframe.presentMicrophoneAccessRequired(from: view)
    }

    func didRequireCameraAccess() {
        wireframe.presentCameraAccessRequired(from: view)
    }

    func didFailVideoCapture() {
        wireframe.presentVideoCaptureFailed(from: view)
    }

    func didUpdateConnectedAt(_ date: Date?) {
        view?.didUpdateConnectedAt(date)
    }

    func didEndCall() {
        wireframe.close(from: view)
    }

    func didReceiveRemoteRenderer(model: ChatCallRendererModel) {
        view?.didReceiveRemoteRenderer(model: model)
    }

    func didReceiveLocalRenderer(model: ChatCallRendererModel) {
        view?.didReceiveLocalRenderer(model: model)
    }

    func didUpdateAudioRoute(_ state: CallAudioRouteState) {
        view?.didUpdateAudioRoute(state)
    }

    func didUpdateMuteState(_ muted: Bool) {
        view?.didUpdateMuteState(muted)
    }

    func didUpdateVideoState(_ isEnabled: Bool) {
        view?.didUpdateVideoState(isEnabled)
    }

    func didReceiveCapability(_ capability: ChatCallCapability) {
        view?.didReceiveCapability(capability)
    }

    func didUpdateRemoteMediaState(_ state: CallRemoteMediaState) {
        view?.didUpdateRemoteMediaState(state)
    }
}

private extension ChatCallPresenter {
    func provideViewModel() {
        let avatarText = peer.name.prefix(1).uppercased()
        let viewModel = ChatCallViewLayout.ViewModel(
            username: peer.name,
            avatarViewModel: .colored(text: avatarText, colorSeed: peer.accountId.toHex()),
            callType: callType,
            isIncoming: role == .acceptor
        )
        view?.didReceive(viewModel: viewModel)
    }
}
