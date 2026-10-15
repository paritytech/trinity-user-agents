import Foundation
import UIKit

#if targetEnvironment(simulator)
    /// The call actions of the truapi E2E harness (S72 to S74). Each action runs the code path of the press that it
    /// stands for.
    extension ChatProductRoomPresenter {
        /// Runs a call action of the harness. Gives false for an action that is not a call action.
        func runCallE2E(_ action: TrUAPIE2EHostAction) -> Bool {
            switch action.action {
            case "call-button": pressCallButton(video: action.argument == "video")
            case "call-answer": answerCall(accept: action.argument != "decline")
            case "call-open": openCall()
            case "call-end": endCall()
            case "call-timer": markCallTimer()
            default: return false
            }
            return true
        }

        /// Presses the call button of the header, when the header shows it.
        private func pressCallButton(video: Bool) {
            let title = String(localized: video ? .videoCall : .audioCall)
            let items = view?.controller.navigationItem.rightBarButtonItems ?? []
            guard items.contains(where: { $0.primaryAction?.title == title }),
                  let screen = view?.controller as? ChatViewController else {
                mark("call-button", "missing video=\(video)")
                return
            }

            mark("call-button", "pressed video=\(video)")
            screen.presenter.startCall(video ? .video : .audio)
        }

        /// Writes the time that the call bar shows.
        private func markCallTimer() {
            guard case let .active(_, _, startedAt, _) = call else {
                mark("call-timer", "none")
                return
            }
            mark("call-timer", ChatRoomCallViewModel.elapsedText(since: Date.fromChatTimestamp(startedAt)))
        }
    }
#endif
