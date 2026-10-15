import UIKit

#if targetEnvironment(simulator)
    /// The performance row B1 of the truapi E2E harness (P7.1, 04-TESTING §5): the list scrolls to its end at a
    /// fixed speed, the product gives its pages on the way, and the frame meter records each frame.
    extension ChatParticipantsViewController {
        func observeE2EActions() -> NSObjectProtocol? {
            guard TrUAPIE2EMarkers.isEnabled else { return nil }

            return NotificationCenter.default.addObserver(
                forName: TrUAPIE2EHostAction.notification,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let action = notification.object as? TrUAPIE2EHostAction,
                      action.action == "participants-scroll" else { return }
                MainActor.assumeIsolated { self?.scrollToEnd(action) }
            }
        }

        /// Scrolls `target` points in each frame (300 when the action gives none) until the list shows
        /// `argument` rows and its last row. The scroll stops after two minutes.
        private func scrollToEnd(_ action: TrUAPIE2EHostAction) {
            guard view.window != nil else { return }

            let expectedRows = action.argument.flatMap(Int.init) ?? 0
            let step = action.target.flatMap(Double.init).map { CGFloat($0) } ?? 300
            let deadline = ContinuousClock.now + .seconds(120)
            let footprintBefore = TrUAPIE2EFrameMeter.footprintMegabytes()
            let meter = TrUAPIE2EFrameMeter()
            e2eMeter = meter

            meter.start(
                onFrame: { [weak self] in
                    guard let self, ContinuousClock.now < deadline else { return false }
                    let table = e2eTableView
                    let bottom = max(
                        0,
                        table.contentSize.height - table.bounds.height + table.adjustedContentInset.bottom
                    )
                    let offset = min(bottom, table.contentOffset.y + step)
                    table.contentOffset.y = offset
                    let loaded = table.numberOfRows(inSection: 0)
                    return !(offset >= bottom && loaded >= expectedRows)
                },
                completion: { [weak self] meter in
                    guard let self else { return }
                    e2eMeter = nil
                    let rows = e2eTableView.numberOfRows(inSection: 0)
                    Logger.shared.debug(
                        "[e2e:ui] perf-participants \(action.room) rows=\(rows) step=\(Int(step)) "
                            + "footprintBeforeMB=\(String(format: "%.1f", footprintBefore)) \(meter.summary)"
                    )
                }
            )
        }
    }
#endif
