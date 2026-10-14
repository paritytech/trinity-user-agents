import Foundation
import UIKit
import AVKit

protocol VideoPreviewPlayerFactoryProtocol {
    @MainActor
    func makePlayerViewController(url: URL) -> AVPlayerViewController
}

final class VideoPreviewPlayerFactory: VideoPreviewPlayerFactoryProtocol {
    private let audioSessionManager: AudioSessionManaging

    init(audioSessionManager: AudioSessionManaging) {
        self.audioSessionManager = audioSessionManager
    }

    @MainActor
    func makePlayerViewController(url: URL) -> AVPlayerViewController {
        let playerViewController = AudioSessionVideoPlayerViewController(
            audioSessionManager: audioSessionManager
        )
        playerViewController.player = AVPlayer(url: url)
        return playerViewController
    }
}
