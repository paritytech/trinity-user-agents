import AVKit

final class AudioSessionVideoPlayerViewController: AVPlayerViewController {
    private let audioSessionManager: AudioSessionManaging

    init(audioSessionManager: AudioSessionManaging) {
        self.audioSessionManager = audioSessionManager
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        try? audioSessionManager.registerActivities([.playback], for: self)
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        try? audioSessionManager.deregisterActivities(for: self)
    }
}
