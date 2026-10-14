import SwiftUI
import DesignSystem

public struct ChatMessageActionView: View, Hashable {
    public let viewModel: ViewModel

    public init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        HStack(alignment: .center, spacing: 16) {
            Group {
                Circle().fill(Color.bgSurfaceNested)
            }
            .aspectRatio(1, contentMode: .fit)
            .frame(width: 48)

            VStack(alignment: .leading, spacing: 4) {
                Text(viewModel.title)
                    .typography(.bodyMediumEmphasized)
                    .foregroundStyle(Color.fgPrimary)

                Text(viewModel.subtitle)
                    .typography(.bodyMedium)
                    .multilineTextAlignment(.leading)
                    .foregroundStyle(Color.fgTertiary)
            }

            Spacer()

            Button(action: viewModel.buttonAction) {
                Text(viewModel.buttonTitle)
                    .typography(.titleSmall)
                    .padding(.horizontal, 16)
            }
            .buttonStyle(.mainDark44)
        }
    }
}

// MARK: - ViewModel

public extension ChatMessageActionView {
    struct ViewModel: Hashable {
        public let title: String
        public let subtitle: String
        public let buttonTitle: String
        public let buttonAction: () -> Void

        public init(
            title: String,
            subtitle: String,
            buttonTitle: String,
            buttonAction: @escaping () -> Void
        ) {
            self.title = title
            self.subtitle = subtitle
            self.buttonTitle = buttonTitle
            self.buttonAction = buttonAction
        }

        public static func == (lhs: ViewModel, rhs: ViewModel) -> Bool {
            lhs.title == rhs.title &&
                lhs.subtitle == rhs.subtitle &&
                lhs.buttonTitle == rhs.buttonTitle
        }

        public func hash(into hasher: inout Hasher) {
            hasher.combine(title)
            hasher.combine(subtitle)
            hasher.combine(buttonTitle)
        }
    }
}

#if DEBUG
    #Preview {
        ChatMessageActionView(viewModel: ChatMessageActionView.ViewModel(
            title: "John Doe",
            subtitle: "Attest humans, remove AI in a 5 minute game",
            buttonTitle: "Open",
            buttonAction: {
                print("View Profile tapped!")
            }
        ))
        .background(Color.bgSurfaceContainer)
        .padding(.horizontal, 16)
    }
#endif
