import DesignSystem
import SwiftUI

public struct DSRadio: View {
    private let isOn: Bool

    public init(isOn: Bool) {
        self.isOn = isOn
    }

    public var body: some View {
        ZStack {
            if isOn {
                Circle()
                    .fill(Color.bgActionPrimary)
                Circle()
                    .fill(Color.fgPrimaryInverted)
                    .frame(width: 8, height: 8)
            } else {
                Circle()
                    .strokeBorder(Color.strokeTertiary, lineWidth: 1)
            }
        }
        .frame(width: 20, height: 20)
    }
}

#if DEBUG
    #Preview("DSRadio") {
        HStack(spacing: 16) {
            DSRadio(isOn: true)
            DSRadio(isOn: false)
        }
        .padding()
        .background(Color.bgSurfaceContainer)
    }
#endif
