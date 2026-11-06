import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// What a finished row's icon draws: a card with an arrow into it or out of
/// it, or a cross. Played from the spinner when the row has just arrived,
/// drawn at rest otherwise. Under Reduce Motion it only fades in.
struct FundingCompletionGlyph: View {
    enum Kind: Equatable {
        case received
        case sent
        case failed
    }

    let kind: Kind
    let animated: Bool

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var stage: Stage

    init(kind: Kind, animated: Bool) {
        self.kind = kind
        self.animated = animated
        _stage = State(initialValue: animated ? .start : .rest)
    }

    var body: some View {
        ZStack {
            if kind == .failed {
                cross
            } else {
                card
                arrow
            }
            loader
        }
        .frame(width: Self.size, height: Self.size)
        .opacity(stage.fade)
        .accessibilityHidden(true)
        .onAppear(perform: play)
    }
}

private extension FundingCompletionGlyph {
    static let size: CGFloat = 22
    static let lineWidth: CGFloat = 1.6

    struct Stage {
        var loader: CGFloat
        var shaft: CGFloat
        var legs: CGFloat
        var arrowScale: CGFloat
        var outline: CGFloat
        var dot: CGFloat
        var crossFirst: CGFloat
        var crossSecond: CGFloat
        var fade: Double

        static let rest = Stage(
            loader: 0, shaft: 1, legs: 1, arrowScale: 1, outline: 1, dot: 1, crossFirst: 1, crossSecond: 1, fade: 1
        )
        static let start = Stage(
            loader: 1,
            shaft: 0,
            legs: 0,
            arrowScale: PurseMotion.Completion.arrowStartScale,
            outline: 0,
            dot: 0,
            crossFirst: 0,
            crossSecond: 0,
            fade: 1
        )
    }

    var direction: FundingDirection {
        kind == .received ? .in : .out
    }

    var card: some View {
        ZStack {
            FundingCardOutline()
                .trim(from: 0, to: stage.outline)
                .stroke(style: StrokeStyle(lineWidth: Self.lineWidth, lineCap: .round))
            FundingCardDot()
                .scaleEffect(stage.dot, anchor: .center)
        }
    }

    var arrow: some View {
        ZStack {
            FundingArrowShaft(direction: direction)
                .trim(from: 0, to: stage.shaft)
                .stroke(style: StrokeStyle(lineWidth: Self.lineWidth, lineCap: .round))
            FundingArrowLegs(direction: direction)
                .trim(from: 0, to: stage.legs)
                .stroke(style: StrokeStyle(lineWidth: Self.lineWidth, lineCap: .round, lineJoin: .round))
        }
        .scaleEffect(stage.arrowScale, anchor: .topTrailing)
    }

    var cross: some View {
        ZStack {
            FundingCrossStroke(rising: false)
                .trim(from: 0, to: stage.crossFirst)
                .stroke(style: StrokeStyle(lineWidth: 2, lineCap: .round))
            FundingCrossStroke(rising: true)
                .trim(from: 0, to: stage.crossSecond)
                .stroke(style: StrokeStyle(lineWidth: 2, lineCap: .round))
        }
        .padding(4)
    }

    var loader: some View {
        Circle()
            .trim(from: 0, to: 0.23 * stage.loader)
            .stroke(Color.fgPrimary, style: StrokeStyle(lineWidth: 2.4, lineCap: .round))
            .frame(width: 21.6, height: 21.6)
            .scaleEffect(0.8 + 0.2 * stage.loader)
            .opacity(Double(stage.loader))
    }

    func play() {
        guard animated else { return }

        if reduceMotion {
            var faded = Stage.rest
            faded.fade = 0
            stage = faded
            withAnimation(.easeOut(duration: PurseMotion.Completion.outline)) { stage.fade = 1 }
            return
        }

        typealias Timing = PurseMotion.Completion
        withAnimation(.easeOut(duration: Timing.loaderOut)) { stage.loader = 0 }
        if kind == .failed {
            withAnimation(.easeOut(duration: Timing.crossStroke).delay(Timing.crossFirstDelay)) { stage.crossFirst = 1 }
            withAnimation(.easeOut(duration: Timing.crossStroke).delay(Timing.crossSecondDelay)) {
                stage.crossSecond = 1
            }
            return
        }
        withAnimation(.easeOut(duration: Timing.shaft).delay(Timing.shaftDelay)) { stage.shaft = 1 }
        withAnimation(.easeOut(duration: Timing.legs).delay(Timing.legsDelay)) { stage.legs = 1 }
        withAnimation(.easeInOut(duration: Timing.shrink).delay(Timing.shrinkDelay)) { stage.arrowScale = 1 }
        withAnimation(.easeOut(duration: Timing.outline).delay(Timing.outlineDelay)) { stage.outline = 1 }
        withAnimation(.spring(response: 0.3, dampingFraction: 0.45).delay(Timing.dotDelay)) { stage.dot = 1 }
    }
}

/// The card's outline, in the glyph's 22-point box.
struct FundingCardOutline: SwiftUI.Shape {
    func path(in rect: CGRect) -> Path {
        Path(roundedRect: Self.card(in: rect), cornerRadius: 2.5)
    }

    static func card(in rect: CGRect) -> CGRect {
        CGRect(
            x: rect.minX + 1,
            y: rect.minY + rect.height * 0.3,
            width: rect.width * 0.72,
            height: rect.height * 0.52
        )
    }
}

/// The dot on the card's lower left.
struct FundingCardDot: SwiftUI.Shape {
    func path(in rect: CGRect) -> Path {
        let card = FundingCardOutline.card(in: rect)
        return Path(ellipseIn: CGRect(x: card.minX + 3, y: card.maxY - 6, width: 3, height: 3))
    }
}

/// The arrow's ends at the card's top-right corner: into the card for value
/// coming in, out of it for value going out.
enum FundingArrowGeometry {
    static func ends(in rect: CGRect, direction: FundingDirection) -> (from: CGPoint, tip: CGPoint) {
        let outer = CGPoint(x: rect.maxX - 1, y: rect.minY + 1)
        let inner = CGPoint(x: rect.minX + rect.width * 0.62, y: rect.minY + rect.height * 0.38)
        return direction == .in ? (outer, inner) : (inner, outer)
    }
}

struct FundingArrowShaft: SwiftUI.Shape {
    let direction: FundingDirection

    func path(in rect: CGRect) -> Path {
        let (from, tip) = FundingArrowGeometry.ends(in: rect, direction: direction)
        var path = Path()
        path.move(to: from)
        path.addLine(to: tip)
        return path
    }
}

struct FundingArrowLegs: SwiftUI.Shape {
    let direction: FundingDirection

    func path(in rect: CGRect) -> Path {
        let (from, tip) = FundingArrowGeometry.ends(in: rect, direction: direction)
        let head = rect.width * 0.18
        let horizontal: CGFloat = tip.x > from.x ? -head : head
        let vertical: CGFloat = tip.y > from.y ? -head : head
        var path = Path()
        path.move(to: CGPoint(x: tip.x + horizontal, y: tip.y))
        path.addLine(to: tip)
        path.addLine(to: CGPoint(x: tip.x, y: tip.y + vertical))
        return path
    }
}

/// One stroke of the failure cross.
struct FundingCrossStroke: SwiftUI.Shape {
    let rising: Bool

    func path(in rect: CGRect) -> Path {
        var path = Path()
        if rising {
            path.move(to: CGPoint(x: rect.minX, y: rect.maxY))
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY))
        } else {
            path.move(to: CGPoint(x: rect.minX, y: rect.minY))
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        }
        return path
    }
}

/// A finished session arriving at the top of Today: it fades in, sharpens and
/// settles from just above. Under Reduce Motion it only fades in.
struct FundingArrival: ViewModifier {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var settled: Bool

    init(isArriving: Bool) {
        _settled = State(initialValue: !isArriving)
    }

    func body(content: Content) -> some View {
        content
            .opacity(settled ? 1 : 0)
            .blur(radius: settled || reduceMotion ? 0 : PurseMotion.Arrival.blur)
            .offset(y: settled || reduceMotion ? 0 : PurseMotion.Arrival.offset)
            .onAppear {
                guard !settled else { return }

                let timing = PurseMotion.Arrival.self
                withAnimation(.easeOut(duration: timing.duration).delay(timing.delay)) { settled = true }
            }
    }
}
