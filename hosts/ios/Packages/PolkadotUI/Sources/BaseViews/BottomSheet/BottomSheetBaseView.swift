import SwiftUI
import DesignSystem

/// SwiftUI version of BottomSheetBaseLayout with matching styling
public struct BottomSheetBaseView<Content: View>: View {
    private let content: Content

    private let cornerRadius: CGFloat = 20
    private let backgroundInsets = EdgeInsets(top: 0, leading: 8, bottom: 8, trailing: 8)
    private let contentInsets = EdgeInsets(top: 16, leading: 16, bottom: 16, trailing: 16)

    public init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    public var body: some View {
        content
            .padding(contentInsets)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius)
                    .fill(Color.bgSurfaceContainer)
            )
            .padding(backgroundInsets)
    }
}

#Preview {
    VStack {
        Spacer()

        BottomSheetBaseView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Title")
                    .typography(.headlineSmall)
                    .foregroundStyle(Color.fgPrimary)

                Text("Some description text")
                    .typography(.paragraphLarge)
                    .foregroundStyle(Color.fgSecondary)
            }
        }
    }
}
