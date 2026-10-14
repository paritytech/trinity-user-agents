import Foundation
import AVFoundation
import WebRTC

struct VideoCaptureParams {
    let format: AVCaptureDevice.Format
    let fps: Int
}

struct VideoCapturePreferences {
    let targetWidth: Int
    let targetHeight: Int
    let targetFps: Int

    init(maxWidth: Int, maxHeight: Int, maxFps: Int) {
        targetWidth = maxWidth
        targetHeight = maxHeight
        targetFps = maxFps
    }

    init(profile: RTCVideoProfile) {
        targetWidth = profile.captureTargetWidth
        targetHeight = profile.captureTargetHeight
        targetFps = profile.captureFps
    }
}

protocol VideoCaptureStrategyProtocol {
    func deriveParams(for device: AVCaptureDevice) -> VideoCaptureParams?
}

///  The video capture strategy ensures that participants of the call uses preferred aspect scale ratio and max fps.
///  That allows stable connection by preventing encoder/decoder overloads on both sides. For example,
///  one device might send ultra high res video while the receiver is too slow to process it or event doesn't support
/// that
///  format.
final class VideoCaptureStrategy {
    let preferences: VideoCapturePreferences
    let logger: LoggerProtocol

    init(preferences: VideoCapturePreferences, logger: LoggerProtocol = Logger.shared) {
        self.preferences = preferences
        self.logger = logger
    }
}

private extension VideoCaptureStrategy {
    func makeCandidate(from format: AVCaptureDevice.Format, id: Int) -> VideoCaptureFormatCandidate {
        let dimensions = CMVideoFormatDescriptionGetDimensions(format.formatDescription)
        let frameRateRanges = format.videoSupportedFrameRateRanges.map {
            $0.minFrameRate ... $0.maxFrameRate
        }

        return VideoCaptureFormatCandidate(
            id: id,
            dimensions: dimensions,
            frameRateRanges: frameRateRanges
        )
    }

    func logSelectedFormat(_ selection: VideoCaptureFormatSelection) {
        logger.debug(
            """
            Video capture format selected: target=\(preferences.targetWidth)x\(preferences.targetHeight)@\(preferences
                .targetFps), \
            selected=\(selection.candidate.dimensions.width)x\(selection.candidate.dimensions.height)@\(selection
                .fps), \
            exactFps=\(selection.supportsTargetFps), fitsDimensions=\(selection.fitsTargetDimensions)
            """
        )
    }
}

extension VideoCaptureStrategy: VideoCaptureStrategyProtocol {
    func deriveParams(for device: AVCaptureDevice) -> VideoCaptureParams? {
        let formats = RTCCameraVideoCapturer.supportedFormats(for: device)
        let candidates = formats.enumerated().map { index, format in
            makeCandidate(from: format, id: index)
        }

        guard let selection = VideoCaptureFormatSelector.select(from: candidates, preferences: preferences) else {
            logger.error("No video capture format available")
            return nil
        }

        if !selection.fitsTargetDimensions {
            logger.warning("No video capture format matching preferred dimensions. Using fallback")
        }

        if !selection.supportsTargetFps {
            logger.warning("No video capture format matching preferred fps. Using fallback")
        }

        logSelectedFormat(selection)

        return VideoCaptureParams(format: formats[selection.candidate.id], fps: selection.fps)
    }
}
