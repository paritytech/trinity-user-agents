import SwiftUI
import DesignSystem

public struct AvatarViewSUI: View {
    private let viewModel: AvatarViewModel
    private let size: CGFloat

    // MARK: - Inits

    public init(viewModel: AvatarViewModel, size: CGFloat) {
        self.viewModel = viewModel
        self.size = size
    }

    // MARK: - Body

    public var body: some View {
        ZStack {
            backgroundColor

            if let image = viewModel.image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
                    .frame(width: size, height: size)
                    .clipped()
            } else if let text = viewModel.text {
                Text(text)
                    .typography(.displayLarge)
                    .foregroundStyle(textColor)
                    .lineLimit(1)
                    .minimumScaleFactor(0.1)
                    .frame(
                        width: size * 0.71,
                        height: size * 0.71,
                        alignment: .center
                    )
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
    }

    // MARK: - Private

    private var backgroundColor: Color {
        if let bgColor = viewModel.backgroundColor {
            Color(bgColor)
        } else {
            Color(.fill12)
        }
    }

    private var textColor: Color {
        if let txtColor = viewModel.textColor {
            Color(txtColor)
        } else {
            Color(.white69)
        }
    }
}

#Preview {
    VStack(spacing: 20) {
        AvatarViewSUI(viewModel: .colored(text: "S", colorSeed: "small"), size: 36)
        AvatarViewSUI(viewModel: .colored(text: "O", colorSeed: "medium"), size: 48)
        AvatarViewSUI(viewModel: .colored(text: "L", colorSeed: "large"), size: 72)
    }
    .padding()
}
