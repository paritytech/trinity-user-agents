import Foundation
import QuartzCore

/// The room screen draws at most one list update in each frame (02-DESIGN §3.5: the host coalesces the `Edited`
/// updates of one message to the refresh rate of the screen; P7.1, B3).
///
/// A product can send a burst of feed updates, for example twenty edits in one millisecond. Each update gives a
/// new list model. The presenter keeps only the newest model and draws it at the next frame, so a burst costs one
/// draw and not twenty.
extension ChatPresenter {
    /// The shortest time between two list updates: one frame of a 60 Hz screen.
    static let listUpdateInterval: CFTimeInterval = 1.0 / 60

    func scheduleListUpdate() {
        guard !listUpdatePending else { return }

        listUpdatePending = true
        let wait = max(0, Self.listUpdateInterval - (CACurrentMediaTime() - lastListUpdate))
        Task { @MainActor [weak self] in
            if wait > 0 {
                try? await Task.sleep(for: .milliseconds(Int(wait * 1_000) + 1))
            }
            guard let self else { return }

            listUpdatePending = false
            lastListUpdate = CACurrentMediaTime()
            applyListModel()
        }
    }
}
