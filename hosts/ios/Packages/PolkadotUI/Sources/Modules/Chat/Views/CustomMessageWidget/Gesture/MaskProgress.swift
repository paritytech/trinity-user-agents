import SwiftUI
import UIKit

public extension CustomMessageWidgetNode {
    /// A feedback the host plays through the device's haptics (the renderer's `Haptic`).
    enum Haptic: Equatable {
        /// A selection tick, as a picker plays between values.
        case tick
        case light
        case medium
        case heavy
        case success
    }
}

public extension CustomMessageWidgetNode.Mask {
    /// Sends the cleared share's action when the cover stops growing short of the share (`ShareRescue`): once it has
    /// not grown for `after` while it is at least `fromPermille`.
    struct Rescue: Equatable {
        public let after: TimeInterval
        public let fromPermille: Int

        public init(after: TimeInterval, fromPermille: some BinaryInteger) {
            self.after = after
            self.fromPermille = Int(fromPermille)
        }
    }

    /// Haptics a mask plays as its cover grows (`MaskHaptics`): `step` each time the cover passes another
    /// `everyPermille`, and `reach` when its cleared share is reached or rescued.
    struct Haptics: Equatable {
        public let everyPermille: Int
        public let step: CustomMessageWidgetNode.Haptic
        public let reach: CustomMessageWidgetNode.Haptic?

        public init(
            everyPermille: some BinaryInteger,
            step: CustomMessageWidgetNode.Haptic,
            reach: CustomMessageWidgetNode.Haptic? = nil
        ) {
            self.everyPermille = Int(everyPermille)
            self.step = step
            self.reach = reach
        }

        /// How many steps lie between a cover of `from` and one of `to`, in thousandths
        /// (`MaskHaptics::steps_passed`).
        public func stepsPassed(from: Int, to: Int) -> Int {
            guard everyPermille > 0, to > from else { return 0 }
            return to / everyPermille - from / everyPermille
        }
    }
}

/// What a mask's cover did since the tree, for its rescue and its haptic steps: when it last grew, and the share the
/// last step was counted from. A new tree starts both over from the tree's own cover.
struct MaskCoverWatch: Equatable {
    /// Covered samples at the last measure.
    private(set) var covered = 0
    /// The cover, in thousandths, steps are counted from.
    private(set) var permille = 0
    /// When the cover last grew: the tree's arrival, or a shape that covered a new sample.
    private(set) var grew: TimeInterval = 0
    private(set) var rescued = false

    init(covered: Int = 0, total: Int = 0, at now: TimeInterval = 0) {
        tree(covered: covered, total: total, at: now)
    }

    mutating func tree(covered: Int, total: Int, at now: TimeInterval) {
        self.covered = covered
        permille = Self.permille(covered, total)
        grew = now
        rescued = false
    }

    /// The cover measured at `now`: whether a haptic step is due for it.
    mutating func measured(
        covered: Int, total: Int, at now: TimeInterval, haptics: CustomMessageWidgetNode.Mask.Haptics?
    ) -> Bool {
        if covered > self.covered {
            self.covered = covered
            grew = now
        }
        let next = Self.permille(covered, total)
        let step = (haptics?.stepsPassed(from: permille, to: next) ?? 0) > 0
        permille = max(permille, next)
        return step
    }

    /// When the rescue is due, if the cover is high enough and nothing was sent this tree.
    func rescueDue(
        share: CustomMessageWidgetNode.Mask.ClearedShare
    ) -> TimeInterval? {
        guard let rescue = share.rescue, !rescued, rescue.fromPermille <= share.atPermille,
              permille >= rescue.fromPermille else { return nil }
        return grew + rescue.after
    }

    /// The action was sent this tree, by the share or the rescue: no rescue follows.
    mutating func sent() {
        rescued = true
    }

    private static func permille(_ covered: Int, _ total: Int) -> Int {
        total > 0 ? covered * 1_000 / total : 0
    }
}

/// Plays a haptic from the device's own set. UIKit plays nothing where the person turned system haptics off.
enum HapticPlayer {
    @MainActor
    static func play(_ haptic: CustomMessageWidgetNode.Haptic) {
        switch haptic {
        case .tick: UISelectionFeedbackGenerator().selectionChanged()
        case .light: UIImpactFeedbackGenerator(style: .light).impactOccurred()
        case .medium: UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        case .heavy: UIImpactFeedbackGenerator(style: .heavy).impactOccurred()
        case .success: UINotificationFeedbackGenerator().notificationOccurred(.success)
        }
    }
}
