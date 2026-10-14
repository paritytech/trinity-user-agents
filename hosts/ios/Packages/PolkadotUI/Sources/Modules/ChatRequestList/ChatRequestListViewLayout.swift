import DesignSystem
import ExternalAccessibility
import SwiftUI

public struct ChatRequestListViewLayout: View {
    @Bindable var viewModel: ChatRequestListViewModel
    let dateFormatter: TimestampFormatting

    public init(viewModel: ChatRequestListViewModel, dateFormatter: TimestampFormatting) {
        self.viewModel = viewModel
        self.dateFormatter = dateFormatter
    }

    public var body: some View {
        TimelineView(.periodic(from: .now, by: viewModel.autoupdateInterval)) { context in
            ScrollView(.vertical) {
                LazyVStack(spacing: 8) {
                    ForEach(viewModel.items) { item in
                        ChatRequestCellView(
                            item: item,
                            date: dateFormatter.string(for: item.date, now: context.date)
                        )
                        .contentShape(Rectangle())
                        .onTapGesture {
                            viewModel.onItemSelection?(item.id)
                        }
                        .accessibilityId(AccessibilityID.Chats.requestRow)

                        Rectangle()
                            .fill(Color.strokeSecondary)
                            .frame(height: 1)
                            .padding(.leading, 72)
                    }
                }
            }
            .contentMargins(.horizontal, 16)
            .contentMargins(.top, 8)
        }
    }
}

private struct ChatRequestCellView: View {
    let item: ChatRequestListItem
    let date: String

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            AvatarViewSUI(viewModel: item.avatarViewModel, size: 56)

            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .bottom) {
                    Text(item.contactName)
                        .typography(.titleMedium)
                        .foregroundStyle(Color.fgPrimary)
                        .lineLimit(1)

                    Spacer()

                    Text(date)
                        .typography(.bodyMedium)
                        .foregroundStyle(Color.fgSecondary)
                        .lineLimit(1)
                        .layoutPriority(1)
                }
                .padding(0)

                HStack(alignment: .top, spacing: 8) {
                    Text(item.messageText)
                        .typography(.paragraphLarge)
                        .foregroundStyle(item.isSeen ? Color.fgSecondary : Color.fgPrimary)
                        .lineLimit(2)

                    Spacer()

                    if !item.isSeen {
                        Image(.unseenRequest)
                    }
                }
                .padding(0)
            }
        }
    }
}

#if DEBUG
    #Preview(traits: .fixedLayout(width: 375, height: 812)) {
        let viewModel = ChatRequestListViewModel()
        viewModel.items = [
            ChatRequestListItem(
                id: "1",
                contactName: "Alice.42",
                avatarViewModel: AvatarViewModel.colored(text: "A", colorSeed: "1"),
                messageText: "Hello! We met at the party last Sunday!",
                date: Date().addingTimeInterval(-3_600),
                isSeen: false
            ),
            ChatRequestListItem(
                id: "2",
                contactName: "Bob.87",
                avatarViewModel: AvatarViewModel.colored(text: "B", colorSeed: "1"),
                messageText: "Message is hidden",
                date: Date().addingTimeInterval(-7_200),
                isSeen: true
            ),
            ChatRequestListItem(
                id: "3",
                contactName: "Alice.42",
                avatarViewModel: AvatarViewModel.colored(text: "A", colorSeed: "2"),
                messageText: "Hey Alice! It's Bob",
                date: Date().addingTimeInterval(-3_600),
                isSeen: false
            ),
        ]
        return ChatRequestListViewLayout(viewModel: viewModel, dateFormatter: TimestampFormatter())
            .background(Color.bgSurfaceMain)
    }
#endif
