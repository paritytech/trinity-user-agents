import SwiftUI
import PolkadotUI

struct GameRoomPillView: View, Hashable {
    private let viewModel: GameRoomPillViewModel

    private enum Layout {
        static let horizontalInset: CGFloat = 36
        static let bottomInset: CGFloat = 8
    }

    init(
        viewModel: GameRoomPillViewModel
    ) {
        self.viewModel = viewModel
    }

    static func == (lhs: GameRoomPillView, rhs: GameRoomPillView) -> Bool {
        lhs.viewModel.content == rhs.viewModel.content
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(viewModel.content)
    }

    var body: some View {
        Button(action: viewModel.onTap) {
            HStack(spacing: 0) {
                Text(viewModel.title)
                    .textStyle(.title18SemiBold())
                    .foregroundStyle(Color(.yellow))
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)

                Spacer(minLength: 2)

                HStack(spacing: 4) {
                    valueView

                    Image(systemName: "chevron.up")
                        .font(.system(size: 16, weight: .bold))
                        .foregroundStyle(Color(.textAndIconsPrimaryLight))
                        .frame(width: 36, height: 36)
                        .background(Color(.yellow), in: Circle())
                }
            }
            .padding(.leading, 16)
            .padding([.vertical, .trailing], 8)
            .frame(maxWidth: .infinity)
            .overlay(
                Capsule()
                    .stroke(Color(.yellow), lineWidth: 2)
            )
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .background(Color(.backgroundPrimary), in: Capsule())
        .padding(.horizontal, Layout.horizontalInset)
        .padding(.bottom, Layout.bottomInset)
    }

    @ViewBuilder
    private var valueView: some View {
        switch viewModel.content {
        case .waiting:
            TimelineView(.periodic(from: .now, by: 1)) { context in
                valueText(viewModel.valueString(now: context.date))
            }

        case .live,
             .finished:
            valueText(viewModel.staticValueString)
        }
    }

    private func valueText(_ value: String) -> some View {
        Text(value)
            .textStyle(.title18SemiBold())
            .monospacedDigit()
            .minimumScaleFactor(0.7)
            .lineLimit(1)
            .foregroundStyle(Color(.textAndIconsPrimaryLight))
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .background(Color(.yellow), in: Capsule())
    }
}

#Preview {
    GameRoomPillView(
        viewModel: .init(content: .live(.init(currentRound: 2, totalRounds: 5)))
    )
}
