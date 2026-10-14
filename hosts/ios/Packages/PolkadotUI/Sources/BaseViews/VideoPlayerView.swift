import UIKit
import AVFoundation

public class VideoPlayerView: UIView {
    override public static var layerClass: AnyClass { AVPlayerLayer.self }

    private var player: AVPlayer? {
        get { playerLayer?.player }
        set { playerLayer?.player = newValue }
    }

    private var playerLayer: AVPlayerLayer? { layer as? AVPlayerLayer }
    private var playerStatusObserver: NSKeyValueObservation?
    private let activityIndicator: UIActivityIndicatorView = .create { view in
        view.style = .medium
        view.color = .white
        view.hidesWhenStopped = true
    }

    deinit {
        clearPlayerObservers()
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        playerLayer?.videoGravity = .resizeAspectFill
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupPlayerObservers() {
        playerStatusObserver = player?.observe(\.status, options: [.new, .initial]) { [weak self] player, _ in
            DispatchQueue.main.async {
                switch player.status {
                case .readyToPlay:
                    self?.activityIndicator.stopAnimating()
                case .unknown:
                    self?.activityIndicator.startAnimating()
                case .failed:
                    self?.activityIndicator.stopAnimating()
                @unknown default:
                    break
                }
            }
        }
    }

    private func clearPlayerObservers() {
        playerStatusObserver?.invalidate()
    }

    private func setupLayout() {
        addSubview(activityIndicator)
        activityIndicator.snp.makeConstraints { make in
            make.center.equalToSuperview()
        }
    }

    public func startLoading() {
        activityIndicator.startAnimating()
    }

    public func stopLoading() {
        activityIndicator.stopAnimating()
    }

    public func play(url: URL) {
        clearPlayerObservers()

        let player = AVPlayer(url: url)

        self.player = player

        setupPlayerObservers()
        activityIndicator.startAnimating()

        player.play()
    }

    public func pause() {
        player?.pause()
    }

    public func resume() {
        player?.play()
    }
}
