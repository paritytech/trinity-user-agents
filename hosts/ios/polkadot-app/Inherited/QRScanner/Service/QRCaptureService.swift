import Foundation
import AVFoundation

protocol QRCaptureServiceProtocol: AnyObject {
    var delegate: QRCaptureServiceDelegate? { get set }
    var delegateQueue: DispatchQueue { get set }

    func start()
    func stop()
}

enum QRCaptureServiceError: Error {
    case deviceAccessDeniedPreviously
    case deviceAccessDeniedNow
    case deviceAccessRestricted
    case unsupportedFormat
}

/// One code the camera read. `text` is nil for a code with no string form.
struct CapturedCode {
    let type: AVMetadataObject.ObjectType
    let text: String?
}

protocol QRCaptureServiceDelegate: AnyObject {
    func qrCapture(service: QRCaptureServiceProtocol, didSetup captureSession: AVCaptureSession)
    /// Every code in one frame, in the order the camera reports them.
    func qrCapture(service: QRCaptureServiceProtocol, didReceive codes: [CapturedCode])
    func qrCapture(service: QRCaptureServiceProtocol, didFailure error: Error)
}

final class QRCaptureService: NSObject {
    static let processingQueue = DispatchQueue(label: "nova.qr.capture.service.queue")

    private(set) var captureSession: AVCaptureSession?
    /// Read and written on `processingQueue`, which also delivers metadata. Armed exactly while the
    /// session runs: flipped in the same block that starts or stops it, so a frame delivered in
    /// between cannot yield a code after the stop was asked for. Filtering here instead of clearing
    /// `metadataObjectTypes` avoids reconfiguring the running session, which would stall the preview.
    private var isRecognitionArmed = false

    weak var delegate: QRCaptureServiceDelegate?
    var delegateQueue: DispatchQueue
    private let metadataObjectTypes: [AVMetadataObject.ObjectType]

    init(
        delegate: QRCaptureServiceDelegate?,
        delegateQueue: DispatchQueue? = nil,
        metadataObjectTypes: [AVMetadataObject.ObjectType] = [.qr]
    ) {
        self.delegate = delegate
        self.delegateQueue = delegateQueue ?? QRCaptureService.processingQueue
        self.metadataObjectTypes = metadataObjectTypes

        super.init()
    }

    private func configureSessionIfNeeded() throws {
        guard self.captureSession == nil else {
            return
        }

        let discoverySession = AVCaptureDevice.DiscoverySession(
            deviceTypes: [.builtInWideAngleCamera],
            mediaType: .video,
            position: .back
        )
        let device = discoverySession.devices.first

        guard let camera = device else {
            throw QRCaptureServiceError.deviceAccessRestricted
        }

        guard let input = try? AVCaptureDeviceInput(device: camera) else {
            throw QRCaptureServiceError.deviceAccessRestricted
        }

        let output = AVCaptureMetadataOutput()

        let captureSession = AVCaptureSession()
        captureSession.addInput(input)
        captureSession.addOutput(output)

        self.captureSession = captureSession

        output.setMetadataObjectsDelegate(self, queue: QRCaptureService.processingQueue)
        output.metadataObjectTypes = metadataObjectTypes
    }

    private func startAuthorizedSession() {
        QRCaptureService.processingQueue.async {
            do {
                try self.configureSessionIfNeeded()

                if let captureSession = self.captureSession {
                    self.isRecognitionArmed = true

                    captureSession.startRunning()

                    self.notifyDelegateWithCreation(of: captureSession)
                }
            } catch {
                self.notifyDelegate(with: error)
            }
        }
    }

    private func notifyDelegate(with error: Error) {
        run(in: delegateQueue) {
            self.delegate?.qrCapture(service: self, didFailure: error)
        }
    }

    private func notifyDelegateWithCreation(of captureSession: AVCaptureSession) {
        run(in: delegateQueue) {
            self.delegate?.qrCapture(service: self, didSetup: captureSession)
        }
    }

    private func notifyDelegate(with codes: [CapturedCode]) {
        run(in: delegateQueue) {
            self.delegate?.qrCapture(service: self, didReceive: codes)
        }
    }

    private func run(in _: DispatchQueue, block: @escaping () -> Void) {
        if delegateQueue != QRCaptureService.processingQueue {
            delegateQueue.async {
                block()
            }
        } else {
            block()
        }
    }

    deinit {
        QRCaptureService.processingQueue.async { [captureSession] in
            captureSession?.stopRunning()
        }
    }
}

extension QRCaptureService: QRCaptureServiceProtocol {
    func start() {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            startAuthorizedSession()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { granted in
                if granted {
                    self.startAuthorizedSession()
                } else {
                    self.notifyDelegate(with: QRCaptureServiceError.deviceAccessDeniedNow)
                }
            }
        case .denied:
            notifyDelegate(with: QRCaptureServiceError.deviceAccessDeniedPreviously)
        case .restricted:
            notifyDelegate(with: QRCaptureServiceError.deviceAccessRestricted)
        @unknown default:
            break
        }
    }

    func stop() {
        QRCaptureService.processingQueue.async {
            self.isRecognitionArmed = false

            self.captureSession?.stopRunning()
        }
    }
}

extension QRCaptureService: AVCaptureMetadataOutputObjectsDelegate {
    func metadataOutput(
        _: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from _: AVCaptureConnection
    ) {
        guard isRecognitionArmed else {
            return
        }

        let codes = metadataObjects
            .compactMap { $0 as? AVMetadataMachineReadableCodeObject }
            .map { CapturedCode(type: $0.type, text: $0.stringValue) }

        guard !codes.isEmpty else {
            return
        }

        notifyDelegate(with: codes)
    }
}
