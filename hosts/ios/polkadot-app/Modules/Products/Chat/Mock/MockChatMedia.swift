#if TESTNET_FEATURE
    import AVFoundation
    import PolkadotUI
    import UIKit

    /// The pictures, icons, avatars, file and video of the mock product. The app draws them, so the showcase needs
    /// no network and no bundled asset, and each stays far below the 1 MiB inline bound of the core.
    struct MockChatMedia: Sendable {
        let pictures: [Data]
        let video: Data?
        let document: Data

        enum Failure: Error {
            case videoWriter
            case videoFrame
        }

        /// A BlurHash that the host draws before the bytes of an item load.
        static let placeholder = "LEHV6nWB2yk8pyo0adR*.7kCMdnj"

        static let pictureSize = CGSize(width: 640, height: 400)

        /// The media of the showcase. The video is nil when this device cannot encode one.
        static func make() async -> MockChatMedia {
            let gradients: [(UIColor, UIColor)] = [
                (.systemPink, .systemPurple),
                (.systemTeal, .systemBlue),
                (.systemOrange, .systemRed),
                (.systemGreen, .systemMint)
            ]
            let pictures = gradients.enumerated().map { picture("Image \($0 + 1)", colors: $1) }

            return await MockChatMedia(
                pictures: pictures,
                video: try? video(),
                document: document()
            )
        }

        /// The images of the trees that the mock product draws, for any source.
        static let widgetImages = WidgetImageResolver { source in
            UIImage(data: picture(String(describing: source), colors: (.systemIndigo, .systemCyan)))
        }

        static func picture(_ title: String, colors: (UIColor, UIColor), size: CGSize = pictureSize) -> Data {
            let renderer = UIGraphicsImageRenderer(size: size, format: opaqueFormat)
            return renderer.jpegData(withCompressionQuality: 0.7) { context in
                let gradient = CGGradient(
                    colorsSpace: CGColorSpaceCreateDeviceRGB(),
                    colors: [colors.0.cgColor, colors.1.cgColor] as CFArray,
                    locations: [0, 1]
                )
                if let gradient {
                    context.cgContext.drawLinearGradient(
                        gradient,
                        start: .zero,
                        end: CGPoint(x: size.width, y: size.height),
                        options: []
                    )
                }
                draw(title, in: CGRect(origin: .zero, size: size), fontSize: size.height / 6)
            }
        }

        /// A white SF Symbol on a clear background, as a toolbar or menu icon.
        static func icon(_ symbolName: String) -> Data {
            let size = CGSize(width: 48, height: 48)
            let configuration = UIImage.SymbolConfiguration(pointSize: 32, weight: .semibold)
            let symbol = UIImage(systemName: symbolName, withConfiguration: configuration)?
                .withTintColor(.white, renderingMode: .alwaysOriginal)

            return UIGraphicsImageRenderer(size: size).pngData { _ in
                guard let symbol else { return }
                let origin = CGPoint(x: (size.width - symbol.size.width) / 2, y: (size.height - symbol.size.height) / 2)
                symbol.draw(at: origin)
            }
        }

        /// A coloured circle with an SF Symbol, as the avatar of a participant or a room. A symbol and not a
        /// letter, so it cannot be taken for the letter avatar that the host draws when an image does not load.
        static func avatar(_ symbolName: String, color: UIColor) -> Data {
            let size = CGSize(width: 96, height: 96)
            let configuration = UIImage.SymbolConfiguration(pointSize: 44, weight: .bold)
            let symbol = UIImage(systemName: symbolName, withConfiguration: configuration)?
                .withTintColor(.white, renderingMode: .alwaysOriginal)

            return UIGraphicsImageRenderer(size: size).pngData { context in
                color.setFill()
                context.cgContext.fillEllipse(in: CGRect(origin: .zero, size: size))
                guard let symbol else { return }
                symbol.draw(at: CGPoint(
                    x: (size.width - symbol.size.width) / 2,
                    y: (size.height - symbol.size.height) / 2
                ))
            }
        }

        /// An inline `data:` address, the form that a persona icon takes.
        static func dataURL(_ data: Data, mimeType: String = "image/png") -> String {
            "data:\(mimeType);base64,\(data.base64EncodedString())"
        }

        /// A one-page PDF, the file of the showcase.
        static func document() -> Data {
            let bounds = CGRect(x: 0, y: 0, width: 595, height: 842)
            return UIGraphicsPDFRenderer(bounds: bounds).pdfData { context in
                context.beginPage()
                let text = """
                Chat Showcase

                This file is a File media item of a RichText body. The host opens it in the document \
                preview, from which the user can share it.
                """
                let attributes: [NSAttributedString.Key: Any] = [.font: UIFont.systemFont(ofSize: 18)]
                (text as NSString).draw(in: bounds.insetBy(dx: 48, dy: 64), withAttributes: attributes)
            }
        }
    }

    private extension MockChatMedia {
        static let videoSide = 240
        static let videoFrames = 30
        static let videoFramesPerSecond: Int32 = 15

        static var opaqueFormat: UIGraphicsImageRendererFormat {
            let format = UIGraphicsImageRendererFormat()
            format.opaque = true
            format.scale = 1
            return format
        }

        static func draw(_ text: String, in rect: CGRect, fontSize: CGFloat) {
            let paragraph = NSMutableParagraphStyle()
            paragraph.alignment = .center
            let attributes: [NSAttributedString.Key: Any] = [
                .font: UIFont.systemFont(ofSize: fontSize, weight: .bold),
                .foregroundColor: UIColor.white,
                .paragraphStyle: paragraph
            ]
            let height = (text as NSString).size(withAttributes: attributes).height
            let line = CGRect(x: rect.minX, y: rect.midY - height / 2, width: rect.width, height: height)
            (text as NSString).draw(in: line, withAttributes: attributes)
        }

        /// Two seconds of H.264: a circle that moves over a colour that changes.
        static func video() async throws -> Data {
            let url = FileManager.default.temporaryDirectory.appending(path: "chat-showcase-\(UUID().uuidString).mp4")
            defer { try? FileManager.default.removeItem(at: url) }

            let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
            let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
                AVVideoCodecKey: AVVideoCodecType.h264,
                AVVideoWidthKey: videoSide,
                AVVideoHeightKey: videoSide
            ])
            input.expectsMediaDataInRealTime = false
            let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32ARGB,
                kCVPixelBufferWidthKey as String: videoSide,
                kCVPixelBufferHeightKey as String: videoSide
            ])
            writer.add(input)
            guard writer.startWriting() else { throw writer.error ?? Failure.videoWriter }
            writer.startSession(atSourceTime: .zero)

            for frame in 0 ..< videoFrames {
                while !input.isReadyForMoreMediaData {
                    try await Task.sleep(for: .milliseconds(5))
                }
                let buffer = try videoFrame(frame, pool: adaptor.pixelBufferPool)
                let time = CMTime(value: CMTimeValue(frame), timescale: videoFramesPerSecond)
                guard adaptor.append(buffer, withPresentationTime: time) else { throw Failure.videoWriter }
            }

            input.markAsFinished()
            await writer.finishWriting()
            guard writer.status == .completed else { throw writer.error ?? Failure.videoWriter }

            return try Data(contentsOf: url)
        }

        static func videoFrame(_ frame: Int, pool: CVPixelBufferPool?) throws -> CVPixelBuffer {
            var buffer: CVPixelBuffer?
            guard let pool, CVPixelBufferPoolCreatePixelBuffer(nil, pool, &buffer) == kCVReturnSuccess,
                  let buffer else {
                throw Failure.videoFrame
            }

            CVPixelBufferLockBaseAddress(buffer, [])
            defer { CVPixelBufferUnlockBaseAddress(buffer, []) }

            guard let context = CGContext(
                data: CVPixelBufferGetBaseAddress(buffer),
                width: videoSide,
                height: videoSide,
                bitsPerComponent: 8,
                bytesPerRow: CVPixelBufferGetBytesPerRow(buffer),
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue
            ) else {
                throw Failure.videoFrame
            }

            let progress = CGFloat(frame) / CGFloat(videoFrames)
            let side = CGFloat(videoSide)
            context.setFillColor(UIColor(hue: progress, saturation: 0.6, brightness: 0.9, alpha: 1).cgColor)
            context.fill(CGRect(x: 0, y: 0, width: side, height: side))
            context.setFillColor(UIColor.white.cgColor)
            context.fillEllipse(in: CGRect(x: progress * (side - 60), y: side / 2 - 30, width: 60, height: 60))

            return buffer
        }
    }
#endif
