import UIKit
import SwiftUI
import PolkadotUI
import WebRTC

final class ChatCallViewController: UIHostingController<ChatCallViewLayout> {
    let presenter: ChatCallPresenterProtocol

    init(presenter: ChatCallPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: ChatCallViewLayout())
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setupUI()
        setupHandlers()
        presenter.setup()
    }

    override var preferredStatusBarStyle: UIStatusBarStyle {
        .lightContent
    }

    private func setupUI() {
        view.backgroundColor = .bgSurfaceMain
        // Hide navigation bar for full-screen call experience
        navigationController?.setNavigationBarHidden(true, animated: false)
    }

    private func setupHandlers() {
        rootView.viewModel.onAcceptCall = { [weak self] in
            self?.presenter.acceptCall()
        }
        rootView.viewModel.onEndCall = { [weak self] in
            self?.presenter.endCall()
        }
        rootView.viewModel.onToggleMute = { [weak self] in
            self?.presenter.toggleMute()
        }
        rootView.viewModel.onToggleVideo = { [weak self] in
            self?.presenter.toggleVideo()
        }
        rootView.viewModel.onSelectAudioRoute = { [weak self] route in
            self?.presenter.selectAudioRoute(route)
        }
    }
}

extension ChatCallViewController: ChatCallViewProtocol {
    func didReceive(viewModel: ChatCallViewLayout.ViewModel) {
        rootView.viewModel.username = viewModel.username
        rootView.viewModel.avatarViewModel = viewModel.avatarViewModel
        rootView.viewModel.callType = viewModel.callType
        rootView.viewModel.isIncoming = viewModel.isIncoming
    }

    func didUpdateCallState(_ state: ChatCallState) {
        rootView.viewModel.callState = state
    }

    func didUpdateConnectedAt(_ date: Date?) {
        rootView.viewModel.connectedAt = date
    }

    func didReceiveRemoteRenderer(model: ChatCallRendererModel) {
        rootView.viewModel.remoteRenderingModel = model
    }

    func didReceiveLocalRenderer(model: ChatCallRendererModel) {
        rootView.viewModel.localRenderingModel = model
    }

    func didUpdateAudioRoute(_ state: CallAudioRouteState) {
        rootView.viewModel.audioRouteState = state
    }

    func didUpdateMuteState(_ muted: Bool) {
        rootView.viewModel.isMuted = muted
    }

    func didUpdateVideoState(_ isEnabled: Bool) {
        rootView.viewModel.isVideoEnabled = isEnabled
    }

    func didReceiveCapability(_ capability: ChatCallCapability) {
        rootView.viewModel.capability = capability
    }

    func didUpdateRemoteMediaState(_ state: CallRemoteMediaState) {
        rootView.viewModel.remoteMediaState = state
    }
}
