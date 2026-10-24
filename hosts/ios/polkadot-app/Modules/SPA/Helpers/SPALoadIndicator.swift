import Foundation
import Products
import UIKit

@MainActor
final class SPALoadIndicator {
    private enum Constants {
        static let statusRevealDelay: Duration = .seconds(3)
        static let resolvingProgress: Float = 0.05
        static let downloadStart: Float = 0.1
        static let downloadSpan: Float = 0.8
        static let unpackingProgress: Float = 0.9
        static let unknownSizeHalfwayBytes: Double = 2_000_000
        static let finishHideDelay: TimeInterval = 0.3
    }

    private let progressView: UIProgressView
    private let statusLabel: UILabel

    private var statusText: String?
    private var canRevealStatus = false
    private var revealTask: Task<Void, Never>?

    init(progressView: UIProgressView, statusLabel: UILabel) {
        self.progressView = progressView
        self.statusLabel = statusLabel
    }

    deinit {
        revealTask?.cancel()
    }

    func start() {
        revealTask?.cancel()
        canRevealStatus = false
        statusText = nil
        applyStatus()

        revealTask = Task { [weak self] in
            try? await Task.sleep(for: Constants.statusRevealDelay)

            guard !Task.isCancelled else { return }

            self?.canRevealStatus = true
            self?.applyStatus()
        }
    }

    func update(_ phase: ProductLoadPhase) {
        if let progress = progress(for: phase) {
            progressView.isHidden = false
            progressView.setProgress(max(progress, progressView.progress), animated: true)
        }

        statusText = status(for: phase)
        applyStatus()
    }

    func finish() {
        stopStatus()

        guard !progressView.isHidden else { return }

        progressView.setProgress(1, animated: true)

        DispatchQueue.main.asyncAfter(deadline: .now() + Constants.finishHideDelay) { [progressView] in
            progressView.isHidden = true
            progressView.setProgress(0, animated: false)
        }
    }

    func stop() {
        stopStatus()
        progressView.isHidden = true
        progressView.setProgress(0, animated: false)
    }
}

private extension SPALoadIndicator {
    func progress(for phase: ProductLoadPhase) -> Float? {
        switch phase {
        case .resolving:
            Constants.resolvingProgress
        case let .downloading(downloadedBytes, totalBytes):
            Constants.downloadStart + Constants.downloadSpan * downloadFraction(downloadedBytes, of: totalBytes)
        case .unpacking:
            Constants.unpackingProgress
        case .waitingForNetwork,
             .retrying:
            nil
        }
    }

    func downloadFraction(_ downloadedBytes: Int64, of totalBytes: Int64?) -> Float {
        let downloaded = Double(downloadedBytes)

        guard let totalBytes, totalBytes > 0 else {
            return Float(downloaded / (downloaded + Constants.unknownSizeHalfwayBytes))
        }

        return Float(min(downloaded / Double(totalBytes), 1))
    }

    func status(for phase: ProductLoadPhase) -> String? {
        switch phase {
        case .resolving,
             .unpacking:
            nil
        case let .downloading(downloadedBytes, _):
            String(localized: .Products.loadStatusDownloading(
                size: ByteCountFormatter.string(fromByteCount: downloadedBytes, countStyle: .file)
            ))
        case .waitingForNetwork:
            String(localized: .Products.loadStatusWaitingForNetwork)
        case .retrying:
            String(localized: .Products.loadStatusRetrying)
        }
    }

    func applyStatus() {
        statusLabel.text = statusText
        statusLabel.isHidden = !canRevealStatus || statusText == nil
    }

    func stopStatus() {
        revealTask?.cancel()
        revealTask = nil
        canRevealStatus = false
        statusText = nil
        applyStatus()
    }
}
