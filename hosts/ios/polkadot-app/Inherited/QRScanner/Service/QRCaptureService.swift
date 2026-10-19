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

protocol QRCaptureServiceDelegate: AnyObject {
    func qrCapture(service: QRCaptureServiceProtocol, didSetup captureSession: AVCaptureSession)
    func qrCapture(service: QRCaptureServiceProtocol, didReceive code: String)
    func qrCapture(service: QRCaptureServiceProtocol, didFailure error: Error)
    /// A code and its type. By default the delegate gets the code only.
    func qrCapture(service: QRCaptureServiceProtocol, didReceive code: String, type: AVMetadataObject.ObjectType)
}

extension QRCaptureServiceDelegate {
    func qrCapture(service: QRCaptureServiceProtocol, didReceive code: String, type _: AVMetadataObject.ObjectType) {
        qrCapture(service: service, didReceive: code)
    }
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
    /// The code types that the camera reads. The types that the device cannot read are left out.
    let codeTypes: [AVMetadataObject.ObjectType]

    init(
        delegate: QRCaptureServiceDelegate?,
        delegateQueue: DispatchQueue? = nil,
        codeTypes: [AVMetadataObject.ObjectType] = [.qr]
    ) {
        self.delegate = delegate
        self.delegateQueue = delegateQueue ?? QRCaptureService.processingQueue
        self.codeTypes = codeTypes

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

        guard let camera = device, let input = try? AVCaptureDeviceInput(device: camera) else {
            throw QRCaptureServiceError.deviceAccessRestricted
        }

        let output = AVCaptureMetadataOutput()

        let captureSession = AVCaptureSession()
        captureSession.addInput(input)
        captureSession.addOutput(output)

        self.captureSession = captureSession

        output.setMetadataObjectsDelegate(self, queue: QRCaptureService.processingQueue)
        output.metadataObjectTypes = codeTypes.filter(output.availableMetadataObjectTypes.contains)
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

    private func notifyDelegateWithCode(_ code: String, type: AVMetadataObject.ObjectType) {
        run(in: delegateQueue) {
            self.delegate?.qrCapture(service: self, didReceive: code, type: type)
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
        guard isRecognitionArmed, let metadata = metadataObjects.first as? AVMetadataMachineReadableCodeObject else {
            return
        }

        guard let possibleCode = metadata.stringValue else {
            notifyDelegate(with: QRCaptureServiceError.unsupportedFormat)
            return
        }

        notifyDelegateWithCode(possibleCode, type: metadata.type)
    }
}
