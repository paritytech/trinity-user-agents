import SwiftUI
import DesignSystem

public struct ChatMessageFileCell: View, Hashable {
    public static let reuseIdentifier = "ChatMessageFileCell"

    public let viewModel: ViewModel

    public init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        VStack(spacing: 0) {
            metadataView

            if let text = viewModel.text {
                textView(with: text)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { viewModel.onTap() }
    }
}

private extension ChatMessageFileCell {
    @ViewBuilder
    var metadataView: some View {
        HStack(alignment: .center, spacing: 12) {
            if let preview = viewModel.preview {
                AsyncImageView(
                    viewModel: preview,
                    settings: ImageViewModelSettings(
                        targetSize: CGSize(width: 64, height: 64)
                    )
                )
                .frame(width: 64, height: 64)
                .background(Color(.white))
                .clipShape(RoundedRectangle(cornerRadius: 16))
            }

            VStack(alignment: .leading, spacing: 8) {
                Text(viewModel.name)
                    .typography(.titleMedium)
                    .foregroundColor(Color(.textAndIconsPrimaryDark))
                    .lineLimit(1)

                if let size = viewModel.size {
                    Text(size)
                        .typography(.paragraphLarge)
                        .foregroundColor(Color(.textAndIconsTertiaryDark))
                        .lineLimit(1)
                }
            }

            Spacer(minLength: 0)
        }
        .padding(16)
        .background(Color(.fill8))
    }

    @ViewBuilder
    func textView(with text: String) -> some View {
        Text(text)
            .typography(.paragraphLarge)
            .foregroundColor(Color(.textAndIconsPrimaryDark))
            .padding(16)
            .background(Color(.backgroundTertiary))
    }
}

public extension ChatMessageFileCell {
    struct ViewModel: Hashable {
        let preview: PreviewImageViewModel?
        let name: String
        let size: String?
        let text: String?
        let onTap: () -> Void

        public init(
            name: String,
            preview: PreviewImageViewModel?,
            size: String?,
            text: String?,
            onTap: @escaping () -> Void
        ) {
            self.name = name
            self.preview = preview
            self.size = size
            self.text = text
            self.onTap = onTap
        }

        public static func == (lhs: ViewModel, rhs: ViewModel) -> Bool {
            lhs.preview == rhs.preview &&
                lhs.name == rhs.name &&
                lhs.size == rhs.size &&
                lhs.text == rhs.text
        }

        public func hash(into hasher: inout Hasher) {
            hasher.combine(preview)
            hasher.combine(name)
            hasher.combine(size)
            hasher.combine(text)
        }
    }
}

#if DEBUG
    #Preview(traits: .sizeThatFitsLayout) {
        let viewModel1 = ChatMessageFileCell.ViewModel(
            name: "Manual.PDF",
            preview: nil,
            size: "390 KB",
            text: "Here’re additional PDF instructions how to provide photo and video evidence",
            onTap: {}
        )

        let viewModel2 = ChatMessageFileCell.ViewModel(
            name: "Manual.PDF",
            preview: nil,
            size: "390 KB",
            text: nil,
            onTap: {}
        )

        VStack(spacing: 20) {
            ChatMessageFileCell(viewModel: viewModel1)
            ChatMessageFileCell(viewModel: viewModel2)
        }
        .background(.red)
    }
#endif
