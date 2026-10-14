import UIKit
import AVKit
internal import SnapKit

public final class AttachmentSelectionVideoView: UIView {
    private weak var parentViewController: UIViewController?
    private var playerViewController: AVPlayerViewController?

    override public init(frame: CGRect) {
        super.init(frame: frame)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func configure(url: URL, parentViewController: UIViewController) {
        self.parentViewController = parentViewController

        let player = AVPlayer(url: url)
        let playerVC = AVPlayerViewController()
        playerVC.player = player
        playerVC.showsPlaybackControls = true
        playerVC.videoGravity = .resizeAspect
        playerVC.view.backgroundColor = .clear

        parentViewController.addChild(playerVC)
        addSubview(playerVC.view)
        playerVC.view.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
        playerVC.didMove(toParent: parentViewController)

        playerViewController = playerVC
    }

    public func setPlaybackControlsHidden(_ hidden: Bool) {
        playerViewController?.showsPlaybackControls = !hidden
    }

    public func cleanup() {
        playerViewController?.player?.pause()
        playerViewController?.willMove(toParent: nil)
        playerViewController?.view.removeFromSuperview()
        playerViewController?.removeFromParent()
        playerViewController = nil
    }
}
