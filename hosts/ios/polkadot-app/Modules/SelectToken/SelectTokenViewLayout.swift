import SwiftUI
import PolkadotUI

struct SelectTokenViewLayout: View {
    @State var viewModel: SelectTokenViewModelProtocol = SelectTokenViewModel()

    var body: some View {
        ScrollView(.vertical) {
            Section {
                LazyVStack(spacing: 8) {
                    ForEach(viewModel.viewModels, id: \.self) { model in
                        Button {
                            viewModel.onTap?(model)
                        } label: {
                            TokenRow(model: model)
                        }
                    }
                }
            } header: {
                Text(.selectCurrencyMainTitle)
                    .textStyle(.title32SemiBold())
                    .foregroundStyle(.textAndIconsPrimaryDark)
                    .frame(maxWidth: .infinity)
                    .padding(.bottom, 40)
            }
        }
        .contentMargins(.horizontal, 16)
    }
}

private extension SelectTokenViewLayout.TokenRow {
    init(model: SelectTokenCellViewModel) {
        switch model {
        case let .chainAsset(chainAsset):
            self.init(icon: chainAsset.icon, label: chainAsset.symbol)
        case .fiat:
            self.init(
                icon: StaticImageViewModel(image: UIImage(resource: .creditCardIcon)),
                label: String(localized: "fiat.onramp.credit.card.label")
            )
        }
    }
}

private extension SelectTokenViewLayout {
    struct TokenRow: View {
        let icon: (any ImageViewModelProtocol)?
        let label: String

        var body: some View {
            HStack(spacing: 16) {
                Group {
                    if let icon {
                        AsyncImageView(viewModel: icon)
                    } else {
                        Circle().fill(.textAndIconsPrimaryDark)
                    }
                }
                .aspectRatio(1, contentMode: .fit)
                .frame(width: 48)

                Text(label)
                    .textStyle(.title18SemiBold())
                    .foregroundStyle(.textAndIconsPrimaryDark)

                Spacer()

                Image(.iconArrowRight)
            }
            .foregroundStyle(.textAndIconsPrimaryDark)
            .padding(16)
            .background(.fill12, in: RoundedRectangle(cornerRadius: 24))
        }
    }
}
