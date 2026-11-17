import CoreGraphics

/// The Docklet's resting frame and the chrome's bounds, the inputs every Path derives from.
struct DockletGeometry: Equatable {
    /// The bar's resting frame.
    let dockFrame: CGRect
    /// The chrome's full-bleed bounds.
    let bounds: CGRect

    /// The transform for a view whose anchor point is `anchor`, when the bar's resting frame is mapped to the screen
    /// by `mapping`.
    func transform(mapping: CGAffineTransform, anchor: CGPoint) -> CGAffineTransform {
        let mappedAnchor = anchor.applying(mapping)
        return CGAffineTransform(
            a: mapping.a,
            b: mapping.b,
            c: mapping.c,
            d: mapping.d,
            tx: mappedAnchor.x - anchor.x,
            ty: mappedAnchor.y - anchor.y
        )
    }

    /// The lowest y of the resting frame's corners once `mapping` maps them to the screen.
    func lowestY(mapping: CGAffineTransform) -> CGFloat {
        [dockFrame.minX, dockFrame.maxX].reduce(-CGFloat.infinity) { lowest, cornerX in
            [dockFrame.minY, dockFrame.maxY].reduce(lowest) {
                max($0, CGPoint(x: cornerX, y: $1).applying(mapping).y)
            }
        }
    }
}
