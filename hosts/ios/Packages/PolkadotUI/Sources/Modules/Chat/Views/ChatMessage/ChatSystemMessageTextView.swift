import SwiftUI
import DesignSystem

public struct ChatSystemMessageTextView: View, Hashable {
    public static let reuseIdentifier = "ChatSystemMessageTextView"

    public let viewModel: ViewModel

    public init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        switch viewModel {
        case let .text(string):
            text(string)
        case let .parts(array):
            parts(array)
        }
    }
}

// MARK: - Subviews

private extension ChatSystemMessageTextView {
    @ViewBuilder
    func text(_ text: AttributedString) -> some View {
        Text(text)
            .typography(.bodyMedium)
            .foregroundColor(Color(.textAndIconsSecondary))
            .multilineTextAlignment(.center)
    }

    @ViewBuilder
    func parts(_ parts: [Part]) -> some View {
        parts
            .map { part in
                switch part {
                case let .plain(string):
                    Text(string)
                        .font(Font(UIFont.bodyMedium))
                        .foregroundColor(Color(.textAndIconsSecondary))
                case let .bold(string):
                    Text(string)
                        .font(Font(UIFont.bodyMediumEmphasized))
                        .foregroundColor(Color(.textAndIconsSecondary))
                }
            }
            .reduce(Text(verbatim: ""), +)
            .multilineTextAlignment(.center)
    }
}

// MARK: - ViewModel

public extension ChatSystemMessageTextView {
    enum ViewModel: Hashable {
        case text(AttributedString)
        case parts([Part])

        public static func text(_ string: String) -> Self {
            .text(AttributedString.from(markdown: string))
        }
    }

    enum Part: Hashable {
        case plain(String)
        case bold(String)
    }
}

// MARK: - Previews

#if DEBUG
    #Preview {
        VStack(spacing: 20) {
            ChatSystemMessageTextView(
                viewModel: .text("Regular System Message")
            )
        }
        .padding()
    }
#endif
