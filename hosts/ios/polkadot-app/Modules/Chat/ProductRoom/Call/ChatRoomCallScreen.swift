import PolkadotUI
import SwiftUI
import UIKit

/// The call screen of the host for a call that a session of the Media service carries (07-LATER §11.3). This build
/// has no Media service (O29), so the screen is a shell: the avatars of the participants, the timer and End. It says
/// that media is not available yet.
struct ChatRoomCallScreen: View {
    let title: String
    let participants: [ChatRoomCallViewModel.Caller]
    let startedAt: Date
    let onEnd: () -> Void

    var body: some View {
        VStack(spacing: 24) {
            Spacer()

            HStack(spacing: -12) {
                ForEach(Array(participants.enumerated()), id: \.offset) { _, participant in
                    ChatRoomCallAvatar(initials: participant.initials, side: 72)
                        .overlay(Circle().stroke(Color.bgSurfaceMain, lineWidth: 3))
                }
            }

            Text(title)
                .typography(.headlineSmall)
                .foregroundStyle(Color.fgPrimary)
                .multilineTextAlignment(.center)

            TimelineView(.periodic(from: .now, by: 1)) { context in
                Text(ChatRoomCallViewModel.elapsedText(since: startedAt, now: context.date))
                    .typography(.bodyLarge)
                    .monospacedDigit()
                    .foregroundStyle(Color.fgSecondary)
            }

            Text(String(localized: .ChatCall.screenMediaUnavailable))
                .typography(.paragraphMedium)
                .foregroundStyle(Color.fgTertiary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)

            Spacer()

            ChatRoomCallRoundButton(
                systemImage: "phone.down.fill",
                background: .bgStatusError,
                label: String(localized: .ChatCall.barEnd),
                action: onEnd
            )
            .scaleEffect(1.4)
            .padding(.bottom, 48)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.bgSurfaceMain)
    }
}

@MainActor
extension ChatRoomCallScreen {
    /// Shows the screen over `controller`. End sends the end of the call and closes the screen.
    static func present(
        from controller: UIViewController,
        title: String,
        participants: [ChatRoomCallViewModel.Caller],
        startedAt: Date,
        onEnd: @escaping () -> Void
    ) {
        weak var hosting: UIViewController?
        let screen = ChatRoomCallScreen(title: title, participants: participants, startedAt: startedAt) {
            onEnd()
            hosting?.dismiss(animated: true)
        }
        let hostingController = UIHostingController(rootView: screen)
        hostingController.modalPresentationStyle = .fullScreen
        hosting = hostingController
        controller.present(hostingController, animated: true)
    }
}
