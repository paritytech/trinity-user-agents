import DesignSystem
import SwiftUI

public struct DSCaption: View {
    private let text: String

    public init(_ text: String) {
        self.text = text
    }

    public var body: some View {
        Text(text)
            .typography(.labelMedium.emphasized)
            .tracking(1)
            .foregroundStyle(Color.fgTertiary)
            .textCase(.uppercase)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, DSSpacings.tiny)
    }
}

#if DEBUG
    #Preview("DSCaption") {
        VStack(alignment: .leading, spacing: 16) {
            DSCaption("general")
            DSCaption("security & privacy")
            DSCaption("legal")
        }
        .padding()
        .background(Color.bgSurfaceMain)
    }
#endif
