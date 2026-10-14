import SwiftUI

public struct HolographicWordmarkView: View {
    let image: ImageResource
    let widthRatio: CGFloat
    let verticalCenterRatio: CGFloat
    let aspectRatio: CGFloat

    private let shader = HolographicShaders.iridescentShine
    private let innerShadow = InnerShadow()

    public init(
        image: ImageResource,
        widthRatio: CGFloat,
        verticalCenterRatio: CGFloat,
        aspectRatio: CGFloat
    ) {
        self.image = image
        self.widthRatio = widthRatio
        self.verticalCenterRatio = verticalCenterRatio
        self.aspectRatio = aspectRatio
    }

    public var body: some View {
        GeometryReader { geo in
            let width = geo.size.width * widthRatio
            let height = width / aspectRatio
            let center = CGPoint(x: geo.size.width / 2, y: geo.size.height * verticalCenterRatio)

            letterforms
                .motionFillingShader(lagged: true, shader: shader)
                .overlay { innerShadowLayer }
                .frame(width: width, height: height)
                .position(center)
        }
        .allowsHitTesting(false)
    }

    private var letterforms: some View {
        Image(image).resizable()
    }

    private var innerShadowLayer: some View {
        letterforms
            .foregroundStyle(innerShadow.color)
            .mask {
                letterforms
                    .overlay {
                        letterforms
                            .offset(x: innerShadow.offset.width, y: innerShadow.offset.height)
                            .blur(radius: innerShadow.radius)
                            .blendMode(.destinationOut)
                    }
                    .compositingGroup()
            }
            .blendMode(innerShadow.blendMode)
    }
}

private struct InnerShadow {
    let color: Color
    let radius: CGFloat
    let offset: CGSize
    let blendMode: BlendMode

    init(
        color: Color = Color(red: 0.564691, green: 0.665138, blue: 0.847769),
        radius: CGFloat = 1.0,
        offset: CGSize = CGSize(width: 0, height: 1.5),
        blendMode: BlendMode = .multiply
    ) {
        self.color = color
        self.radius = radius
        self.offset = offset
        self.blendMode = blendMode
    }
}
