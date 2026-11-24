import SwiftUI

public extension CustomMessageWidgetNode {
    /// When a cover counts as cleared (`RendererNode.CoverGrid`'s `reveal`): once its cleared cells reach `atPermille`
    /// thousandths of the grid and the finger is up on the pan keyed `pan`, the host sends `action` once for that lift.
    struct CoverReveal: Equatable {
        public let pan: String
        public let atPermille: Int
        public let action: String

        public init(pan: String, atPermille: some BinaryInteger, action: String) {
            self.pan = pan
            self.atPermille = Int(atPermille)
            self.action = action
        }

        /// Whether `cleared` cells of `cells` reach the share, as the core counts it:
        /// `cleared * 1000 >= atPermille * cells`.
        public func reached(cleared: Int, cells: Int) -> Bool {
            cleared * 1_000 >= atPermille * cells
        }
    }
}

/// Whether a finger is on one keyed pan, and how many times it has lifted, for a cover that reveals on a lift. A tree
/// leaves it as it is: the finger has not moved.
@Observable
public final class PanTouch {
    public private(set) var isDown = false
    public private(set) var lifts = 0

    @MainActor
    func set(down: Bool) {
        guard down != isDown else { return }
        isDown = down
        if !down { lifts += 1 }
    }
}

/// Decides when a cover's reveal is sent: after a lift, the first time its cleared cells reach the share, so the cells
/// a worker clears in answer to the pan's end count; never while the finger is down; once per lift.
struct CoverRevealWatch: Equatable {
    struct Input: Equatable {
        let down: Bool
        let lifts: Int
        let cleared: Int
    }

    private var lifts = 0
    private var armed = false

    /// The lifts seen before this cover was drawn, which must not send its reveal.
    init(lifts: Int = 0) {
        self.lifts = lifts
    }

    /// Whether to send the reveal now.
    mutating func update(_ input: Input, cells: Int, reveal: CustomMessageWidgetNode.CoverReveal) -> Bool {
        if input.lifts != lifts {
            lifts = input.lifts
            armed = true
        }
        guard !input.down else {
            armed = false
            return false
        }
        guard armed, reveal.reached(cleared: input.cleared, cells: cells) else { return false }
        armed = false
        return true
    }
}

/// Sends a cover's reveal: it reads the cells cleared since the tree and the pan's touch, so only this empty view
/// draws again when they change, never the cover.
struct CoverRevealWatcher: View {
    let reveal: CustomMessageWidgetNode.CoverReveal
    let props: CustomMessageWidgetNode.CoverGridProps
    let channel: CoverGridChannel?
    let touch: PanTouch?

    @Environment(\.widgetActionHandler) private var actionHandler
    @State private var watch: CoverRevealWatch?

    var body: some View {
        let cells = props.columns * props.rows
        let input = CoverRevealWatch.Input(
            down: touch?.isDown ?? false,
            lifts: touch?.lifts ?? 0,
            cleared: props.cleared.union(channel?.cleared ?? CoverCells()).count(below: cells)
        )
        Color.clear
            .onAppear { watch = CoverRevealWatch(lifts: input.lifts) }
            .onChange(of: input) { _, input in
                var next = watch ?? CoverRevealWatch()
                let send = next.update(input, cells: cells, reveal: reveal)
                watch = next
                if send { actionHandler?(reveal.action, nil) }
            }
    }
}
