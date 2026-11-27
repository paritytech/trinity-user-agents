import DesignSystem
import PolkadotUI
import SwiftUI

/// A QR code on a white tile, `side` points square. The tile stays blank
/// until `image` is drawn.
struct QRCodeTile: View {
    let image: UIImage?
    let side: CGFloat
    var padding: CGFloat = DSSpacings.medium
    var cornerRadius: CGFloat = DSRadii.large

    var body: some View {
        Color.clear
            .overlay {
                if let image {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFit()
                }
            }
            .frame(width: side, height: side)
            .padding(padding)
            .background(.fgStaticWhite, in: RoundedRectangle(cornerRadius: cornerRadius))
    }
}
