import UIKit
import AVFoundation
import UIKit_iOS
import Foundation_iOS
import FoundationExt

class QRScannerViewController: UIViewController, ViewHolder {
    typealias RootViewType = QRScannerViewLayout

    let presenter: QRScannerPresenterProtocol

    var messageVisibilityDuration: TimeInterval = 5.0

    lazy var messageAppearanceAnimator: BlockViewAnimatorProtocol = BlockViewAnimator()
    lazy var messageDissmissAnimator: BlockViewAnimatorProtocol = BlockViewAnimator()

    init(presenter: QRScannerPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    deinit {
        invalidateMessageScheduling()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = QRScannerViewLayout()
    }

    override func viewIsAppearing(_ animated: Bool) {
        super.viewIsAppearing(animated)
        presenter.setup()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        presenter.viewDidAppear()
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        presenter.viewWillDisappear()
    }

    func configureVideoLayer(with captureSession: AVCaptureSession) {
        if let layer = rootView.qrFrameView.frameLayer as? AVCaptureVideoPreviewLayer {
            if layer.session === captureSession {
                return
            }

            layer.session = captureSession
        } else {
            let videoPreviewLayer = AVCaptureVideoPreviewLayer(session: captureSession)
            videoPreviewLayer.videoGravity = AVLayerVideoGravity.resizeAspectFill
            videoPreviewLayer.frame = view.layer.bounds

            rootView.qrFrameView.frameLayer = videoPreviewLayer
        }

        rootView.didAttachPreview()
    }

    // MARK: Message Management

    private func scheduleMessageHide() {
        invalidateMessageScheduling()

        perform(#selector(hideMessage), with: true, afterDelay: messageVisibilityDuration)
    }

    private func invalidateMessageScheduling() {
        NSObject.cancelPreviousPerformRequests(
            withTarget: self,
            selector: #selector(hideMessage),
            object: true
        )
    }

    @objc private func hideMessage() {
        let block: () -> Void = { [weak self] in
            self?.rootView.messageLabel.alpha = 0.0
        }

        messageDissmissAnimator.animate(block: block, completionBlock: nil)
    }
}

extension QRScannerViewController: QRScannerViewProtocol {
    var isCoveredByModal: Bool {
        sequence(first: self, next: \.parent).contains { $0.presentedViewController != nil }
    }

    func didReceive(session: AVCaptureSession) {
        configureVideoLayer(with: session)
    }

    func present(message: String, animated: Bool, autoDismiss: Bool) {
        // A pending hide from an earlier message would otherwise dismiss this one.
        invalidateMessageScheduling()

        rootView.messageLabel.text = message

        let block: () -> Void = { [weak self] in
            self?.rootView.messageLabel.alpha = 1.0
        }

        if animated {
            messageAppearanceAnimator.animate(block: block, completionBlock: nil)
        } else {
            block()
        }

        if autoDismiss {
            scheduleMessageHide()
        }
    }
}
