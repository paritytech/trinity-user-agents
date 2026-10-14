import SwiftUI
import DesignSystem

public struct ReactionDetailsPopupView: View {
    public let viewModel: ReactionDetailsViewModel
    public let timestampFormatter: DateFormatter
    public let onDismiss: () -> Void

    @State private var isPresented = false

    public init(
        viewModel: ReactionDetailsViewModel,
        timestampFormatter: DateFormatter,
        onDismiss: @escaping () -> Void
    ) {
        self.viewModel = viewModel
        self.timestampFormatter = timestampFormatter
        self.onDismiss = onDismiss
    }

    public var body: some View {
        ZStack {
            Color(.black100).opacity(isPresented ? 0.5 : 0)
                .ignoresSafeArea()
                .onTapGesture {
                    dismissWithAnimation()
                }

            ReactionDetailsView(
                viewModel: viewModel,
                timestampFormatter: timestampFormatter,
                onDismiss: dismissWithAnimation
            )
            .scaleEffect(isPresented ? 1 : 0.8)
            .opacity(isPresented ? 1 : 0)
        }
        .onAppear {
            withAnimation(.spring(response: 0.3, dampingFraction: 0.7)) {
                isPresented = true
            }
        }
    }
}

// MARK: - Private functions

extension ReactionDetailsPopupView {
    private func dismissWithAnimation() {
        withAnimation(.easeOut(duration: 0.2)) {
            isPresented = false
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) {
            onDismiss()
        }
    }
}

public struct ReactionDetailsView: View {
    public let viewModel: ReactionDetailsViewModel
    public let timestampFormatter: DateFormatter
    public let onDismiss: () -> Void

    public init(
        viewModel: ReactionDetailsViewModel,
        timestampFormatter: DateFormatter,
        onDismiss: @escaping () -> Void
    ) {
        self.viewModel = viewModel
        self.timestampFormatter = timestampFormatter
        self.onDismiss = onDismiss
    }

    public var body: some View {
        VStack(spacing: 0) {
            header
            reactionsList
        }
        .background(Color(.color232323))
        .cornerRadius(16)
        .frame(maxWidth: 320, maxHeight: 400)
    }
}

// MARK: - Private functions

extension ReactionDetailsView {
    private var header: some View {
        VStack(spacing: 2) {
            HStack {
                Button(action: onDismiss) {
                    Image(systemName: "xmark")
                        .typography(.paragraphSmall)
                        .foregroundColor(Color(.textAndIconsSecondary))
                }
                .frame(width: 24, height: 24)

                Spacer()

                Text(String(localized: .reactionsTitle))
                    .typography(.titleMedium)
                    .foregroundColor(Color(.textAndIconsPrimaryDark))

                Spacer()

                Color.clear
                    .frame(width: 24, height: 24)
            }

            Text(String(localized: .reactionsTotal(viewModel.totalCount)))
                .typography(.paragraphSmall)
                .foregroundColor(Color(.textAndIconsSecondary))
        }
        .padding(.horizontal, 16)
        .padding(.top, 12)
        .padding(.bottom, 8)
    }

    private var reactionsList: some View {
        ScrollView {
            VStack(spacing: 0) {
                ForEach(viewModel.reactions) { group in
                    reactionGroupSection(group)
                }
            }
            .padding(.bottom, 8)
        }
    }

    private func reactionGroupSection(_ group: ReactionDetailsViewModel.ReactionGroup) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            reactionGroupHeader(group)

            ForEach(group.reactors) { reactor in
                reactorRow(reactor)
            }
        }
    }

    private func reactionGroupHeader(_ group: ReactionDetailsViewModel.ReactionGroup) -> some View {
        HStack(spacing: 6) {
            Text(group.emoji)
                .typography(.titleLarge)

            Text(verbatim: "\(group.count)")
                .typography(.titleSmall)
                .foregroundColor(Color(.textAndIconsPrimaryDark))

            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    private func reactorRow(_ reactor: ReactionDetailsViewModel.Reactor) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(Color(.white24))
                .frame(width: 24, height: 24)
                .overlay(
                    Text(String(reactor.username.prefix(1)).uppercased())
                        .typography(.labelSmall)
                        .foregroundColor(Color(.textAndIconsPrimaryDark))
                )

            Text(reactor.username)
                .typography(.bodyMedium)
                .foregroundColor(Color(.textAndIconsPrimaryDark))
                .lineLimit(1)

            Spacer()

            Text(timestampFormatter.string(from: reactor.timestamp))
                .typography(.paragraphSmall)
                .foregroundColor(Color(.textAndIconsSecondary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
    }
}

#Preview {
    let formatter = DateFormatter()
    formatter.dateFormat = "h:mm a"

    return ZStack {
        Color(.black100).opacity(0.5)
            .ignoresSafeArea()

        ReactionDetailsView(
            viewModel: ReactionDetailsViewModel(
                totalCount: 3,
                reactions: [
                    .init(
                        emoji: "👍",
                        count: 2,
                        reactors: [
                            .init(id: "1", username: "valentun.73", timestamp: Date()),
                            .init(id: "2", username: "valentunother.84", timestamp: Date())
                        ]
                    ),
                    .init(
                        emoji: "❤️",
                        count: 1,
                        reactors: [
                            .init(id: "3", username: "valentun.73", timestamp: Date())
                        ]
                    )
                ]
            ),
            timestampFormatter: formatter,
            onDismiss: {}
        )
    }
}
