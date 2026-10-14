import SwiftUI

struct ChatMessageActionList: View, Hashable {
    let actions: [ChatMessageActionView.ViewModel]
    let padding: CGFloat

    init(actions: [ChatMessageActionView.ViewModel], padding: CGFloat = .zero) {
        self.actions = actions
        self.padding = padding
    }

    var body: some View {
        VStack(spacing: 4) {
            ForEach(actions, id: \.hashValue) { viewModel in
                ChatMessageActionView(viewModel: viewModel)
                    .padding(16)
                    .background(Color(.backgroundTertiary))
                    .cornerRadius(16)
            }
        }
        .padding(padding)
    }
}
