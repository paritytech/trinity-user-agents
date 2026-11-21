import SwiftUI
import Testing
@testable import PolkadotUI

/// The thumb follows the finger one for one: the drag divides the finger's move by the same distance
/// the face draws the thumb over. That distance is the lane inside the track's inset less the thumb,
/// so in the room footer's 80, with the thumb 12 from the edges, a full slide is 24 shorter than the
/// control's width less the thumb.
@MainActor
struct SlideToConfirmTravelTests {
    /// The room footer's track on a 402-wide phone: 16 a side for the footer, 8 for the product's padding.
    private static let footerWidth: CGFloat = 354
    private static let footerHeight: CGFloat = 80

    private static func thumbTravel(width: CGFloat, trackHeight: CGFloat) -> CGFloat {
        SlideToConfirmMetrics.travel(of: .thumb, width: width, trackHeight: trackHeight)
    }

    /// Where the face draws the thumb's leading edge, from the lane's start, at a progress.
    private static func drawnOffset(progress: CGFloat, width: CGFloat, trackHeight: CGFloat) -> CGFloat {
        let lane = width - 2 * SlideToConfirmMetrics.thumbInset(trackHeight: trackHeight)
        return progress * SlideToConfirmMetrics.thumbTravel(laneWidth: lane)
    }

    @Test("In the room footer a full slide carries the thumb 274, the lane inside the inset less the thumb")
    func footerTravel() {
        #expect(Self.thumbTravel(width: Self.footerWidth, trackHeight: Self.footerHeight) == 274)
    }

    @Test(
        "A finger move of N points moves the thumb N points before it reaches the end",
        arguments: [(CGFloat(354), CGFloat(80)), (354, 72), (342, 80), (300, 56)]
    )
    func thumbFollowsTheFingerOneForOne(width: CGFloat, trackHeight: CGFloat) {
        let travel = Self.thumbTravel(width: width, trackHeight: trackHeight)
        for move: CGFloat in [1, 10, 100, travel / 2, travel - 1] {
            var track = SlideToConfirmTrack()
            track.drag(translation: move, travel: travel)

            let drawn = Self.drawnOffset(progress: track.progress, width: width, trackHeight: trackHeight)
            #expect(abs(drawn - move) < 0.0001, "moved \(move), drew \(drawn) on \(width)×\(trackHeight)")
        }
    }

    @Test("A drag of the full travel reaches the end, and released there it confirms")
    func fullDragReachesTheEnd() {
        let travel = Self.thumbTravel(width: Self.footerWidth, trackHeight: Self.footerHeight)
        var track = SlideToConfirmTrack()

        track.drag(translation: travel, travel: travel)
        #expect(track.progress == 1)
        let drawn = Self.drawnOffset(progress: track.progress, width: Self.footerWidth, trackHeight: Self.footerHeight)
        #expect(drawn == travel)
        #expect(track.release(lifted: true) == .confirm)
        #expect(track.isHeldAtEnd)
    }

    @Test("The thumb confirms once it is released 90% of its drawn travel along, and not before")
    func thresholdIsOnTheDrawnTravel() {
        let travel = Self.thumbTravel(width: Self.footerWidth, trackHeight: Self.footerHeight)

        // 90% of 274 is 246.6.
        var past = SlideToConfirmTrack()
        past.drag(translation: 247, travel: travel)
        #expect(past.release(lifted: true) == .confirm)

        var short = SlideToConfirmTrack()
        short.drag(translation: 246, travel: travel)
        #expect(short.release(lifted: true) == .springBack)
        #expect(short.progress == 0)
    }

    @Test("A track too narrow for the thumb does not travel")
    func narrowTrackHasNoTravel() {
        #expect(Self.thumbTravel(width: 70, trackHeight: Self.footerHeight) == 0)
        #expect(SlideToConfirmMetrics.travel(of: .reveal, width: 40, trackHeight: Self.footerHeight) == 0)
    }

    @Test("The reveal keeps its own travel: the width less the end it leaves unpeeled")
    func revealKeepsItsOwnTravel() {
        for trackHeight: CGFloat in [Self.footerHeight, 72] {
            #expect(SlideToConfirmMetrics.travel(of: .reveal, width: Self.footerWidth, trackHeight: trackHeight) == 298)
        }
    }

    /// Drawn at the end of a full slide, the thumb's trailing edge sits on the inset, 12 from the
    /// track's end, which is where the travel the drag uses puts it.
    @Test("Drawn slid to the end in the footer, the thumb stops at the inset, 12 from the track's end")
    func drawnThumbEndsAtTheInset() throws {
        let renderer = ImageRenderer(
            content: SlideToConfirmThumbFace(
                props: .init(text: "Slide to register", confirmAction: "register"),
                progress: 1,
                isHinting: false,
                trackHeight: Self.footerHeight
            )
            .frame(width: Self.footerWidth)
        )
        renderer.scale = 1
        let image = try #require(renderer.cgImage)
        let middle = image.height / 2
        let thumbEnd = Int(Self.footerWidth) - 12
        let track = try pixel(of: image, column: thumbEnd + 3, row: middle)

        #expect(image.width == Int(Self.footerWidth))
        #expect(try pixel(of: image, column: thumbEnd - 3, row: middle) != track, "the thumb reaches the inset")
        #expect(try pixel(of: image, column: thumbEnd - 8, row: middle) != track, "the thumb is drawn up to the inset")
        #expect(
            try pixel(of: image, column: thumbEnd - 56 - 3, row: middle) == track,
            "the track shows before the thumb"
        )
    }

    private func pixel(of image: CGImage, column: Int, row: Int) throws -> [UInt8] {
        var pixel: [UInt8] = [0, 0, 0, 0]
        let context = try #require(CGContext(
            data: &pixel,
            width: 1,
            height: 1,
            bitsPerComponent: 8,
            bytesPerRow: 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ))
        context.draw(image, in: CGRect(x: -column, y: row - image.height + 1, width: image.width, height: image.height))
        return pixel
    }
}
