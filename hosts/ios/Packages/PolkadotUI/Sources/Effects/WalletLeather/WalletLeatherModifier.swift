import SwiftUI

// Lights a picture of a leather wallet by the phone's motion, as the coins are lit: the relief of
// the leather and the brass of its button catch a light that moves as the phone does.
//
//   Image(.walletLeather).resizable().scaledToFit().walletLeather(.cashWallet)
//

/// What the shader has to know about the picture: its size in pixels, for the texel step the
/// relief is measured over, and where the brass button is.
public struct WalletLeatherImage: Sendable {
    public let pixelSize: CGSize
    /// The button's centre as a fraction of the picture, and its radius as a fraction of the width.
    public let buttonCentre: CGPoint
    public let buttonRadius: CGFloat

    public init(pixelSize: CGSize, buttonCentre: CGPoint, buttonRadius: CGFloat) {
        self.pixelSize = pixelSize
        self.buttonCentre = buttonCentre
        self.buttonRadius = buttonRadius
    }

    /// `wallet-leather.png`, measured from the reference's baked region map.
    public static let cashWallet = WalletLeatherImage(
        pixelSize: CGSize(width: 1_024, height: 642),
        buttonCentre: CGPoint(x: 0.499, y: 0.495),
        buttonRadius: 0.049
    )

    public var aspectRatio: CGFloat { pixelSize.width / pixelSize.height }
}

public extension View {
    func walletLeather(_ image: WalletLeatherImage, isActive: Bool = true) -> some View {
        modifier(WalletLeatherModifier(image: image, isActive: isActive))
    }
}

struct WalletLeatherModifier: ViewModifier {
    let image: WalletLeatherImage
    let isActive: Bool

    private var motion: CardEffectMotionEngine { .shared }

    /// Where the light sits while the phone is held still, and how far a tilt can move it. The
    /// reference's values: rest is where the creases glint without the leather flooding.
    private static let rest = CGPoint(x: 0.2, y: -0.4)
    private static let amplitude: CGFloat = 0.55

    func body(content: Content) -> some View {
        if isActive {
            content
                .visualEffect { [image, tilt] content, proxy in
                    content.layerEffect(
                        Self.shader(size: proxy.size, image: image, tilt: tilt),
                        // The shadow taps reach 7 texels; a texel is under a point on any phone.
                        maxSampleOffset: CGSize(width: 8, height: 8)
                    )
                }
                .onAppear { motion.retain() }
                .onDisappear { motion.release() }
        } else {
            content
        }
    }

    private var tilt: CGPoint {
        let hand = CGPoint(x: motion.tilt.x, y: -motion.tilt.y)

        return CGPoint(
            x: Self.rest.x + Self.amplitude * hand.x,
            y: Self.rest.y + Self.amplitude * hand.y
        )
    }

    private nonisolated static func shader(size: CGSize, image: WalletLeatherImage, tilt: CGPoint) -> Shader {
        ShaderLibrary.bundle(.module).walletLeather(
            .float2(Float(size.width), Float(size.height)),
            .float2(Float(image.pixelSize.width), Float(image.pixelSize.height)),
            .float2(Float(tilt.x), Float(tilt.y)),
            .float3(Float(image.buttonCentre.x), Float(image.buttonCentre.y), Float(image.buttonRadius))
        )
    }
}
