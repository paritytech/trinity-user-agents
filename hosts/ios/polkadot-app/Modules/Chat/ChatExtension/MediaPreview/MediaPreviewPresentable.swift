import Foundation
import UIKit
import QuickLook
import AVKit
import UniformTypeIdentifiers

@MainActor
protocol MediaPreviewPresentable: AnyObject {
    var mediaPreviewLogger: LoggerProtocol { get }
    var mediaPreviewActiveDataSources: [PhotoPreviewDataSource] { get set }
    var videoPreviewPlayerFactory: VideoPreviewPlayerFactoryProtocol { get }

    func showPhoto(
        url: URL,
        from view: UIViewController?
    )
    func showVideo(
        url: URL,
        from view: UIViewController?
    )
}

extension MediaPreviewPresentable {
    func showMedia(
        url: URL,
        utType: UTType,
        from view: UIViewController?
    ) {
        if utType.conforms(to: .image) {
            showPhoto(url: url, from: view)
        } else if utType.conforms(to: .movie) {
            showVideo(url: url, from: view)
        }
    }
}

extension MediaPreviewPresentable {
    var mediaPreviewLogger: LoggerProtocol { Logger.shared }

    func showPhoto(
        url: URL,
        from view: UIViewController?
    ) {
        guard QLPreviewController.canPreview(url as QLPreviewItem) else {
            mediaPreviewLogger.error("Invalid url: \(url) for preview")
            return
        }

        guard let view else {
            mediaPreviewLogger.warning("view is nil, cannot show photo preview")
            return
        }

        let previewController = QLPreviewController()
        let dataSource = PhotoPreviewDataSource(fileURL: url)
        previewController.dataSource = dataSource
        previewController.delegate = dataSource

        mediaPreviewActiveDataSources.append(dataSource)
        dataSource.onDismiss = { [weak self] dismissedDataSource in
            self?.mediaPreviewActiveDataSources.removeAll { $0 === dismissedDataSource }
        }

        view.present(previewController, animated: true)
    }

    func showVideo(
        url: URL,
        from view: UIViewController?
    ) {
        guard let view else {
            mediaPreviewLogger.warning("view is nil, cannot show video preview")
            return
        }

        let playerViewController = videoPreviewPlayerFactory.makePlayerViewController(url: url)

        view.present(playerViewController, animated: true)
        playerViewController.player?.play()
    }
}

// MARK: - QLPreviewController DataSource

final class PhotoPreviewDataSource: NSObject, QLPreviewControllerDataSource, QLPreviewControllerDelegate {
    let fileURL: URL
    var onDismiss: ((PhotoPreviewDataSource) -> Void)?

    init(fileURL: URL) {
        self.fileURL = fileURL
    }

    func numberOfPreviewItems(in _: QLPreviewController) -> Int {
        1
    }

    func previewController(_: QLPreviewController, previewItemAt _: Int) -> QLPreviewItem {
        fileURL as QLPreviewItem
    }

    func previewControllerDidDismiss(_: QLPreviewController) {
        onDismiss?(self)
    }
}
