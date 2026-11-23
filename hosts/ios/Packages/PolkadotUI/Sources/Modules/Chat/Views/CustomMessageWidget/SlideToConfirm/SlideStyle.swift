import SwiftUI

/// The existing slider's look and motion, value for value: its colours, pill, text lines, thumb, chevron, spinner and
/// measured constants. The native slide to confirm draws with these, so it looks as the existing slider does.
enum SlideStyle {
    /// `--track`
    static let track = Color(.sRGB, red: 0x33 / 255, green: 0x36 / 255, blue: 0x3A / 255)
    /// `--thumb`
    static let thumb = Color(.sRGB, red: 0xFD / 255, green: 0xFD / 255, blue: 0xFD / 255)
    /// `--chevron`
    static let chevron = Color(.sRGB, red: 0x0A / 255, green: 0x0A / 255, blue: 0x0A / 255)
    /// `--line1`
    static let line1 = Color(.sRGB, red: 0x58 / 255, green: 0x5A / 255, blue: 0x62 / 255)
    /// `--line2`
    static let line2 = Color(.sRGB, red: 0xF5 / 255, green: 0xF6 / 255, blue: 0xF9 / 255)
    /// `#thumb` box-shadow: 0 2px 6px rgba(0,0,0,0.36)
    static let shadow = CustomMessageWidgetNode.Shadow(color: .black.opacity(0.36), x: 0, y: 2, blur: 6)
    /// `#text` left and right, from the pill's edges.
    static let textStart: CGFloat = 84
    static let textEnd: CGFloat = 16
    /// `#text` gap and padding-top.
    static let gap: CGFloat = 4.6
    static let textPaddingTop: CGFloat = 1
    /// `#text` font-size 18px, line-height 22px, letter-spacing 0; `#l1` weight 400, `#l2` weight 600.
    static let line1Font = CustomMessageWidgetNode.TextFont(size: 18, lineHeight: 22, weight: 400, color: line1, letterSpacing: 0)
    static let line2Font = CustomMessageWidgetNode.TextFont(size: 18, lineHeight: 22, weight: 600, color: line2, letterSpacing: 0)
    /// `FADE_RATE`: text opacity = 1 - 1.4 * progress.
    static let fadeRate: CGFloat = 1.4
    /// The `#l1` band: its 13 gradient stops, 186 wide, 212 pt/s from 70 before to 70 after, then 1 s at rest.
    static let shimmer = CustomMessageWidgetNode.Shimmer(
        color: .white,
        stops: [0, 0.011, 0.054, 0.181, 0.44, 0.766, 0.97, 0.766, 0.44, 0.181, 0.054, 0.011, 0],
        band: 186,
        lead: 70,
        speed: 212,
        rest: 1
    )
    /// `#spin`: one turn in 0.8 s.
    static let spinPeriod: TimeInterval = 0.8

    /// `PRESS_SCALE`, `PRESS_MS`, the 150 ms return to size, the spring (18 rad/s, 0.8), `APPROACH_MS`, `POP`,
    /// `POP_OFFSET`.
    static let feel: CustomMessageWidgetNode.PanFeel = {
        var feel = CustomMessageWidgetNode.PanFeel()
        feel.pressScale = 1.059
        feel.pressDuration = 0.133
        feel.unpressDuration = 0.15
        feel.springOmega = 18
        feel.springDamping = 0.8
        feel.approachDuration = 0.117
        feel.approachReference = 42
        feel.pop = [1.059, 1.132, 1.192, 1.225, 1.235, 1.237, 1.237, 1.237, 1.232, 1.226, 1.219, 1.198, 1.149, 1.099,
                    1.050, 1.023, 1.005, 0.991, 0.981, 0.977, 0.976, 0.977, 0.983, 0.988, 0.990, 0.994, 0.998, 1.0]
        feel.popOffset = 3.33
        return feel
    }()

    static func textOpacity(progress: CGFloat) -> CGFloat {
        min(max(1 - fadeRate * min(max(progress, 0), 1), 0), 1)
    }
}

/// `#chev`: the chevron path in the thumb's 56-point box, stroked 2.6 with round ends.
struct SlideChevron: Shape {
    func path(in rect: CGRect) -> Path {
        let scale = rect.width / 56
        var path = Path()
        path.move(to: CGPoint(x: rect.minX + 24.63 * scale, y: rect.minY + 21.47 * scale))
        path.addLine(to: CGPoint(x: rect.minX + 31.37 * scale, y: rect.minY + 28 * scale))
        path.addLine(to: CGPoint(x: rect.minX + 24.63 * scale, y: rect.minY + 34.53 * scale))
        return path
    }
}

/// `#spin circle`: radius 9, stroked 2.4 with round ends, dashed 34 on and 60 off from three o'clock, turning.
struct SlideSpinner: View {
    let tint: Color

    var body: some View {
        let circumference = 2 * CGFloat.pi * 9
        Circle()
            .trim(from: 0, to: min(34 / circumference, 1))
            .stroke(tint, style: StrokeStyle(lineWidth: 2.4, lineCap: .round))
            .frame(width: 18, height: 18)
            .modifier(NodeSpinModifier(period: SlideStyle.spinPeriod))
    }
}
