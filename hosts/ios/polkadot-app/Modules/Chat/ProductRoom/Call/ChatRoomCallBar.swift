import PolkadotUI
import SwiftUI

/// The state of the call view of a room screen. The room presenter sets the call, and the view draws it.
@MainActor
final class ChatRoomCallBarModel: ObservableObject {
    @Published var viewModel: ChatRoomCallViewModel?

    var onAccept: () -> Void = {}
    var onDecline: () -> Void = {}
    var onOpen: () -> Void = {}
    var onEnd: () -> Void = {}
}

/// The ringing banner and the call bar at the top of a product room (07-LATER §11.2).
struct ChatRoomCallBar: View {
    @ObservedObject var model: ChatRoomCallBarModel

    var body: some View {
        if let viewModel = model.viewModel {
            Group {
                switch viewModel.state {
                case let .ringing(caller):
                    ringing(caller: caller, video: viewModel.video)
                case let .active(startedAt, _):
                    active(startedAt: startedAt, video: viewModel.video)
                }
            }
            .padding(12)
            .background(Color.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: 16))
            .padding(.horizontal, 12)
            .transition(.move(edge: .top).combined(with: .opacity))
        }
    }

    private func ringing(caller: ChatRoomCallViewModel.Caller, video: Bool) -> some View {
        HStack(spacing: 12) {
            ChatRoomCallAvatar(initials: caller.initials, side: 40)

            VStack(alignment: .leading, spacing: 2) {
                Text(caller.name)
                    .typography(.titleSmall)
                    .foregroundStyle(Color.fgPrimary)
                    .lineLimit(1)
                Text(ChatRoomCallViewModel.ringingTitle(video: video))
                    .typography(.bodySmall)
                    .foregroundStyle(Color.fgSecondary)
                    .lineLimit(1)
            }
            .accessibilityElement(children: .combine)

            Spacer(minLength: 8)

            ChatRoomCallRoundButton(
                systemImage: "phone.down.fill",
                background: .bgStatusError,
                label: String(localized: .ChatCall.ringingDecline),
                action: model.onDecline
            )
            ChatRoomCallRoundButton(
                systemImage: video ? "video.fill" : "phone.fill",
                background: .bgStatusSuccess,
                label: String(localized: .ChatCall.ringingAccept),
                action: model.onAccept
            )
        }
    }

    private func active(startedAt: Date, video: Bool) -> some View {
        HStack(spacing: 12) {
            Button(action: model.onOpen) {
                HStack(spacing: 8) {
                    Image(systemName: video ? "video.fill" : "phone.fill")
                        .foregroundStyle(Color.fgSuccess)
                    Text(ChatRoomCallViewModel.barTitle(video: video))
                        .typography(.titleSmall)
                        .foregroundStyle(Color.fgPrimary)
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        Text(ChatRoomCallViewModel.elapsedText(since: startedAt, now: context.date))
                            .typography(.bodyMedium)
                            .monospacedDigit()
                            .foregroundStyle(Color.fgSecondary)
                    }
                    Spacer(minLength: 8)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityHint(String(localized: .ChatCall.barOpenHint))

            Button(action: model.onEnd) {
                Text(String(localized: .ChatCall.barEnd))
                    .typography(.titleSmall)
                    .foregroundStyle(Color.fgStaticWhite)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(Color.bgStatusError, in: Capsule())
            }
            .buttonStyle(.plain)
        }
    }
}

/// A round button of the ringing banner.
struct ChatRoomCallRoundButton: View {
    let systemImage: String
    let background: Color
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(Color.fgStaticWhite)
                .frame(width: 44, height: 44)
                .background(background, in: Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

/// The avatar of a participant of a call: a circle with the initials.
struct ChatRoomCallAvatar: View {
    let initials: String
    let side: CGFloat

    var body: some View {
        Text(initials)
            .typography(.titleSmall)
            .foregroundStyle(Color.fgPrimary)
            .frame(width: side, height: side)
            .background(Color.bgActionTertiary, in: Circle())
            .accessibilityHidden(true)
    }
}
