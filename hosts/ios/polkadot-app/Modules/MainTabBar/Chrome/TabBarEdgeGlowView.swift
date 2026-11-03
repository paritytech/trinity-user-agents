import DesignSystem
import UIKit

/// The Edge glow on the trailing edge: a band in the inverted surface colour, masked to fade out from the screen edge
/// and round off at both ends. While a finger holds the tucked bar's Grab zone, the band brightens, and a soft shimmer
/// flows along it and swells.
final class TabBarEdgeGlowView: UIView {
    /// Multiple of the base alpha that the boost layer adds, clamped at 1.
    private static let boostMultiplier: CGFloat = 1.9
    /// Multiple of each shimmer colour's HSB saturation.
    private static let saturationMultiplier: CGFloat = 1.3
    /// Opacity of the boost layer while held.
    private static let heldBoostOpacity: Float = 0.6
    /// Opacity of the shimmer while held.
    private static let heldShimmerOpacity: Float = 0.95
    /// Duration of the boost and shimmer rise when a hold begins.
    private static let riseDuration: CFTimeInterval = 0.22
    /// Duration of the release fades, and of the fade out after a settle toward shown starts.
    private static let fadeDuration: CFTimeInterval = 0.32
    /// Time for the shimmer to scroll one glow length.
    private static let shimmerPeriod: CFTimeInterval = 1.0
    /// Time for the counter shimmer to scroll one glow length, against the main shimmer.
    private static let counterShimmerPeriod: CFTimeInterval = 1.6
    /// Opacity of each shimmer layer. The two layers add where they overlap, so the container's opacity sets the
    /// held level.
    private static let layerShimmerOpacity: Float = 0.6
    /// Colours of each shimmer layer visible along the glow at once; the layer scrolls through the rest.
    private static let visibleColorCount = 3
    /// Colours of the shimmer, in the order they run along the glow.
    private static let shimmerHexes = [0x5B8CFF, 0xA78BFA, 0xF472B6, 0xFB923C]
    /// Peak x scale of the shimmer container during a swell. The container is anchored on the right edge, so the
    /// swell reaches in from the edge.
    private static let swellScale: CGFloat = 1.35
    /// Time for the width swell to go out and back once.
    private static let swellWidthPeriod: CFTimeInterval = 1.9
    /// Opacity of each shimmer layer at the dim point of the brightness swell, as a fraction of `layerShimmerOpacity`.
    private static let swellDimFraction: Float = 0.75
    /// Time for the brightness swell to go out and back once.
    private static let swellBrightnessPeriod: CFTimeInterval = 1.3
    private static let fadeKey = "edgeGlowFade"
    private static let shimmerKey = "edgeGlowShimmer"
    private static let counterShimmerKey = "edgeGlowCounterShimmer"
    /// Key of the width swell on the shimmer container.
    private static let swellWidthKey = "edgeGlowSwellWidth"
    /// Key of the brightness swell on each shimmer layer.
    private static let swellBrightnessKey = "edgeGlowSwellBrightness"
    /// Share of the band's width left when the bar is fully shown. The glow narrows toward the edge as the bar comes
    /// out.
    private static let minWidthFraction: CGFloat = 0.5

    private let contentLayer = CALayer()
    private let baseLayer = CALayer()
    private let baseMask = CALayer()
    private let boostLayer = CALayer()
    private let boostMask = CALayer()
    private let shimmerContainer = CALayer()
    private let shimmerLayer = CAGradientLayer()
    private let counterShimmerLayer = CAGradientLayer()
    private let shimmerMask = CALayer()
    private var maskedSize = CGSize.zero
    private var maskedScale: CGFloat = 0
    private var isHeld = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupLayers()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        refreshLayers()
    }

    /// Restores the tucked bar when VoiceOver activates the glow, its one accessibility element.
    var onAccessibilityActivate: (() -> Void)?

    override func accessibilityActivate() -> Bool {
        guard let onAccessibilityActivate else {
            return false
        }
        onAccessibilityActivate()
        return true
    }

    /// Sets the view's opacity and width from Hide progress: opacity equals progress, and the width narrows from the
    /// full band at progress 1 to `minWidthFraction` of it at progress 0. Cancels any fade out in progress.
    func setProgress(_ progress: CGFloat) {
        layer.removeAnimation(forKey: Self.fadeKey)
        alpha = progress
        let clampedProgress = min(max(progress, 0), 1)
        let widthScale = Self.minWidthFraction + (1 - Self.minWidthFraction) * clampedProgress
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        contentLayer.transform = CATransform3DMakeScale(widthScale, 1, 1)
        CATransaction.commit()
    }

    /// Fades the view from its current opacity to zero. Does nothing once the view is already faded out.
    func fadeOut() {
        guard alpha > 0 else {
            return
        }
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.fromValue = layer.presentation()?.opacity ?? layer.opacity
        fade.toValue = 0
        fade.duration = Self.fadeDuration
        fade.timingFunction = CAMediaTimingFunction(controlPoints: 0.2, 0.8, 0.2, 1)
        alpha = 0
        layer.add(fade, forKey: Self.fadeKey)
    }

    /// Brightens the glow while a finger holds the Grab zone, and returns it to rest on release. Reduce Motion skips
    /// the shimmer's movement and swell, and keeps the fades.
    func setHeld(_ held: Bool, animated: Bool) {
        isHeld = held
        CATransaction.begin()
        CATransaction.setDisableActions(!animated)
        CATransaction.setAnimationDuration(held ? Self.riseDuration : Self.fadeDuration)
        CATransaction.setCompletionBlock { [weak self] in
            self?.stopShimmerFlowIfReleased()
        }
        boostLayer.opacity = held ? Self.heldBoostOpacity : 0
        shimmerContainer.opacity = held ? Self.heldShimmerOpacity : 0
        CATransaction.commit()

        if held, !UIAccessibility.isReduceMotionEnabled {
            startShimmerFlow()
        }
    }

    /// The colour with its HSB saturation multiplied by `saturationMultiplier`, clamped at 1. A grey stays grey.
    static func saturationBoosted(_ color: UIColor) -> UIColor {
        var hue: CGFloat = 0
        var saturation: CGFloat = 0
        var brightness: CGFloat = 0
        var alpha: CGFloat = 0
        color.getHue(&hue, saturation: &saturation, brightness: &brightness, alpha: &alpha)
        return UIColor(
            hue: hue,
            saturation: min(1, saturation * saturationMultiplier),
            brightness: brightness,
            alpha: alpha
        )
    }
}

private extension TabBarEdgeGlowView {
    /// Shimmer colours in the order they run along a layer: the colour cycle from index `start`, then its first
    /// `visibleColorCount` colours again. Scrolling one cycle length brings the colour one cycle ahead into place,
    /// so the gradient wraps seamlessly.
    static func shimmerCGColors(startingAt start: Int) -> [CGColor] {
        let hues = shimmerHexes.map { saturationBoosted(UIColor(rgbHex: $0)).cgColor }
        let rotated = Array(hues[start...] + hues[..<start])
        return rotated + rotated.prefix(visibleColorCount)
    }

    /// Distance one colour cycle spans along the glow. Neighbouring colours sit `glowHeight / (visibleColorCount - 1)`
    /// apart, so `visibleColorCount` colours fit in the glow.
    var shimmerCycleLength: CGFloat {
        CGFloat(Self.shimmerHexes.count) * bounds.height / CGFloat(Self.visibleColorCount - 1)
    }

    var pixelScale: CGFloat {
        traitCollection.displayScale > 0 ? traitCollection.displayScale : 2
    }

    func setupLayers() {
        isUserInteractionEnabled = false
        layer.addSublayer(contentLayer)
        contentLayer.anchorPoint = CGPoint(x: 1, y: 0.5)
        contentLayer.addSublayer(baseLayer)
        baseLayer.mask = baseMask
        contentLayer.addSublayer(boostLayer)
        boostLayer.mask = boostMask
        boostLayer.opacity = 0

        contentLayer.addSublayer(shimmerContainer)
        shimmerContainer.anchorPoint = CGPoint(x: 1, y: 0.5)
        shimmerContainer.mask = shimmerMask
        shimmerContainer.compositingFilter = "plusL"
        shimmerContainer.opacity = 0
        shimmerContainer.addSublayer(shimmerLayer)
        shimmerContainer.addSublayer(counterShimmerLayer)
        for shimmer in [shimmerLayer, counterShimmerLayer] {
            shimmer.opacity = Self.layerShimmerOpacity
            shimmer.startPoint = CGPoint(x: 0.5, y: 0)
            shimmer.endPoint = CGPoint(x: 0.5, y: 1)
        }
        shimmerLayer.colors = Self.shimmerCGColors(startingAt: 0)
        counterShimmerLayer.compositingFilter = "plusL"
        counterShimmerLayer.colors = Self.shimmerCGColors(startingAt: Self.shimmerHexes.count / 2)

        registerForTraitChanges([DSThemeTrait.self, UITraitUserInterfaceStyle.self]) { (view: TabBarEdgeGlowView, _) in
            view.refreshLayers()
        }
    }

    /// Re-derives the layer frames and colours from the view's size and current traits. CGColor values do not follow
    /// trait changes, so the surface colour is resolved against the traits here.
    func refreshLayers() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        for edgeAnchored in [contentLayer, shimmerContainer] {
            edgeAnchored.bounds = bounds
            edgeAnchored.position = CGPoint(x: bounds.maxX, y: bounds.midY)
        }
        for sublayer in [baseLayer, boostLayer, baseMask, boostMask, shimmerMask] {
            sublayer.frame = bounds
        }
        for shimmer in [shimmerLayer, counterShimmerLayer] {
            shimmer.frame = CGRect(x: 0, y: 0, width: bounds.width, height: bounds.height + shimmerCycleLength)
        }
        rebuildMasksIfNeeded()
        applyColors()
        CATransaction.commit()
    }

    func applyColors() {
        let surface = UIColor.bgSurfaceContainerInverted.resolvedColor(with: traitCollection).cgColor
        baseLayer.backgroundColor = surface
        boostLayer.backgroundColor = surface
    }

    /// Re-renders the masks when the size or the display scale changes. The masks hold alpha only, so trait changes
    /// that alter colour leave them as they are.
    func rebuildMasksIfNeeded() {
        let scale = pixelScale
        guard !bounds.isEmpty, bounds.size != maskedSize || scale != maskedScale else {
            return
        }
        maskedSize = bounds.size
        maskedScale = scale
        let length = bounds.height
        fillMask(baseMask) { DockletGlowProfile.glowAlpha(inFromEdge: $0, along: $1, length: length) }
        fillMask(boostMask) {
            min(1, Self.boostMultiplier * DockletGlowProfile.glowAlpha(inFromEdge: $0, along: $1, length: length))
        }
        fillMask(shimmerMask) { DockletGlowProfile.shimmerStrength(inFromEdge: $0, along: $1, length: length) }
    }

    func fillMask(_ mask: CALayer, alpha: (CGFloat, CGFloat) -> CGFloat) {
        mask.contents = alphaImage(alpha)
        mask.contentsScale = pixelScale
    }

    /// Renders `alpha` over the view's bounds at the display scale. Each pixel is passed its distance in points from
    /// the right edge and its distance in points down the glow.
    func alphaImage(_ alpha: (CGFloat, CGFloat) -> CGFloat) -> CGImage? {
        let scale = pixelScale
        let width = Int((bounds.width * scale).rounded())
        let height = Int((bounds.height * scale).rounded())
        guard let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ), let data = context.data else {
            return nil
        }
        context.clear(CGRect(x: 0, y: 0, width: width, height: height))
        let pixels = data.assumingMemoryBound(to: UInt8.self)
        for row in 0 ..< height {
            let along = (CGFloat(row) + 0.5) / scale
            for column in 0 ..< width {
                let inFromEdge = bounds.width - (CGFloat(column) + 0.5) / scale
                let coverage = min(max(alpha(inFromEdge, along), 0), 1)
                pixels[row * context.bytesPerRow + column * 4 + 3] = UInt8((coverage * 255).rounded())
            }
        }
        return context.makeImage()
    }

    /// Moves the two shimmer layers in opposite directions, and swells the shimmer in width and brightness.
    func startShimmerFlow() {
        guard bounds.height > 0 else {
            return
        }
        let cycle = shimmerCycleLength
        shimmerLayer.add(
            shimmerFlow(from: 0, to: -cycle, period: Self.shimmerPeriod),
            forKey: Self.shimmerKey
        )
        counterShimmerLayer.add(
            shimmerFlow(from: -cycle, to: 0, period: Self.counterShimmerPeriod),
            forKey: Self.counterShimmerKey
        )
        addSwell()
    }

    func shimmerFlow(from start: CGFloat, to end: CGFloat, period: CFTimeInterval) -> CABasicAnimation {
        let flow = CABasicAnimation(keyPath: "transform.translation.y")
        flow.fromValue = start
        flow.toValue = end
        flow.duration = period * CFTimeInterval(abs(end - start) / bounds.height)
        flow.timingFunction = CAMediaTimingFunction(name: .linear)
        flow.repeatCount = .infinity
        return flow
    }

    /// Swells the shimmer container's width from the right edge, and the brightness of both shimmer layers. The two
    /// periods differ, so the swells drift against each other.
    func addSwell() {
        shimmerContainer.add(
            swellAnimation("transform.scale.x", from: 1, to: Self.swellScale, period: Self.swellWidthPeriod),
            forKey: Self.swellWidthKey
        )
        let dimOpacity = Self.layerShimmerOpacity * Self.swellDimFraction
        for shimmer in [shimmerLayer, counterShimmerLayer] {
            shimmer.add(
                swellAnimation(
                    "opacity",
                    from: dimOpacity,
                    to: Self.layerShimmerOpacity,
                    period: Self.swellBrightnessPeriod
                ),
                forKey: Self.swellBrightnessKey
            )
        }
    }

    /// An ease-in-out swing between `start` and `end` that reverses on each leg and repeats forever.
    func swellAnimation(_ keyPath: String, from start: Any, to end: Any, period: CFTimeInterval) -> CABasicAnimation {
        let swell = CABasicAnimation(keyPath: keyPath)
        swell.fromValue = start
        swell.toValue = end
        swell.duration = period
        swell.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        swell.autoreverses = true
        swell.repeatCount = .infinity
        return swell
    }

    /// The release fade ends with all shimmer animations removed, unless a new hold has started meanwhile.
    func stopShimmerFlowIfReleased() {
        guard !isHeld else {
            return
        }
        shimmerLayer.removeAnimation(forKey: Self.shimmerKey)
        shimmerLayer.removeAnimation(forKey: Self.swellBrightnessKey)
        counterShimmerLayer.removeAnimation(forKey: Self.counterShimmerKey)
        counterShimmerLayer.removeAnimation(forKey: Self.swellBrightnessKey)
        shimmerContainer.removeAnimation(forKey: Self.swellWidthKey)
    }
}
