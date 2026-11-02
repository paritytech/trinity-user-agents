import DesignSystem
import UIKit

/// The Edge glow on the trailing edge: a feathered band in the inverted surface colour. While a finger holds the
/// tucked bar's Grab zone, the band brightens and widens, and a shimmer runs along it.
final class TabBarEdgeGlowView: UIView {
    /// Multiple of the base alpha that the boost layer adds, clamped at 1.
    private static let boostMultiplier: CGFloat = 1.9
    /// Multiple of each shimmer colour's HSB saturation.
    private static let saturationMultiplier: CGFloat = 1.3
    /// Opacity of the boost layer while held.
    private static let heldBoostOpacity: Float = 0.6
    /// Opacity of the shimmer while held.
    private static let heldShimmerOpacity: Float = 0.95
    /// Horizontal scale of the band while held, anchored at the screen edge.
    private static let heldBandScale: CGFloat = 1.08
    /// Stiffness of the spring that scales the band out when a hold begins.
    private static let springStiffness: CGFloat = 300
    /// Damping ratio of that spring, which overshoots its scale slightly.
    private static let springDampingRatio: CGFloat = 0.7
    /// Duration of the boost and shimmer rise when a hold begins.
    private static let riseDuration: CFTimeInterval = 0.22
    /// Duration of the band's scale when a hold begins.
    private static let heldScaleDuration: CFTimeInterval = 0.26
    /// Duration of the release fades, and of the fade out after a settle toward shown starts.
    private static let fadeDuration: CFTimeInterval = 0.32
    /// Duration of one pass of the shimmer along the glow.
    private static let shimmerPeriod: CFTimeInterval = 1.4
    /// Colours of the shimmer, in the order they run along the glow.
    private static let shimmerHexes = [0x5B8CFF, 0xA78BFA, 0xF472B6, 0xFB923C, 0xFACC15, 0x34D399, 0x38BDF8]
    private static let fadeKey = "edgeGlowFade"
    private static let scaleKey = "edgeGlowScale"
    private static let shimmerKey = "edgeGlowShimmer"

    private let profileStops = DockletGlowProfile.blurredStops()
    private let shimmerColors = TabBarEdgeGlowView.shimmerCGColors()
    private let bandLayer = CALayer()
    private let baseLayer = CAGradientLayer()
    private let boostLayer = CAGradientLayer()
    private let featherLayer = CAGradientLayer()
    private let shimmerContainer = CALayer()
    private let shimmerLayer = CAGradientLayer()
    private let shimmerMaskLayer = CAGradientLayer()
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

    /// Sets the whole view's opacity, cancelling any fade out in progress.
    func setOpacity(_ opacity: CGFloat) {
        layer.removeAnimation(forKey: Self.fadeKey)
        alpha = opacity
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
    /// the scale and the shimmer's movement, and keeps the fades.
    func setHeld(_ held: Bool, animated: Bool) {
        isHeld = held
        CATransaction.begin()
        CATransaction.setDisableActions(!animated)
        CATransaction.setAnimationDuration(held ? Self.riseDuration : Self.fadeDuration)
        CATransaction.setCompletionBlock { [weak self] in
            self?.stopShimmerFlowIfReleased()
        }
        boostLayer.opacity = held ? Self.heldBoostOpacity : 0
        shimmerLayer.opacity = held ? Self.heldShimmerOpacity : 0
        CATransaction.commit()

        guard !UIAccessibility.isReduceMotionEnabled else {
            return
        }
        setBandScale(held ? Self.heldBandScale : 1, animated: animated, spring: held)
        if held {
            startShimmerFlow()
        }
    }

    /// The colour with its HSB saturation multiplied by 1.3, clamped at 1. A grey stays grey.
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
    /// Shimmer colours for two passes of the cycle, each pass ending on the colour it starts with.
    static func shimmerCGColors() -> [CGColor] {
        let hues = shimmerHexes.map { saturationBoosted(UIColor(rgbHex: $0)).cgColor }
        let cycle = hues + hues.prefix(1)
        return cycle + cycle.dropFirst()
    }

    func setupLayers() {
        isUserInteractionEnabled = false
        layer.addSublayer(bandLayer)
        bandLayer.anchorPoint = CGPoint(x: 1, y: 0.5)
        bandLayer.addSublayer(baseLayer)
        bandLayer.addSublayer(boostLayer)
        boostLayer.opacity = 0

        layer.addSublayer(shimmerContainer)
        shimmerContainer.masksToBounds = true
        shimmerContainer.addSublayer(shimmerLayer)
        shimmerLayer.compositingFilter = "plusL"
        shimmerLayer.opacity = 0
        shimmerLayer.mask = shimmerMaskLayer
        shimmerLayer.colors = shimmerColors
        shimmerMaskLayer.colors = [UIColor.black.cgColor, UIColor.clear.cgColor]

        for horizontal in [baseLayer, boostLayer, shimmerMaskLayer] {
            horizontal.startPoint = CGPoint(x: 1, y: 0.5)
            horizontal.endPoint = CGPoint(x: 0, y: 0.5)
        }
        featherLayer.startPoint = CGPoint(x: 0.5, y: 0)
        featherLayer.endPoint = CGPoint(x: 0.5, y: 1)
        bandLayer.mask = featherLayer

        registerForTraitChanges([DSThemeTrait.self, UITraitUserInterfaceStyle.self]) { (view: TabBarEdgeGlowView, _) in
            view.refreshLayers()
        }
    }

    /// Re-derives the layer frames and colours from the view's size and current traits. CGColor values do not follow
    /// trait changes, so the surface colour is resolved against the traits here.
    func refreshLayers() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        bandLayer.bounds = bounds
        bandLayer.position = CGPoint(x: bounds.maxX, y: bounds.midY)
        for sublayer in [baseLayer, boostLayer, featherLayer, shimmerContainer] {
            sublayer.frame = bounds
        }
        shimmerLayer.frame = CGRect(x: 0, y: 0, width: bounds.width, height: 2 * bounds.height)
        shimmerMaskLayer.frame = shimmerLayer.bounds
        applyColors()
        CATransaction.commit()
    }

    func applyColors() {
        let surface = UIColor.bgSurfaceContainerInverted.resolvedColor(with: traitCollection)
        baseLayer.colors = profileStops.map { surface.withAlphaComponent($0.alpha).cgColor }
        boostLayer.colors = profileStops.map {
            surface.withAlphaComponent(min(1, $0.alpha * Self.boostMultiplier)).cgColor
        }
        baseLayer.locations = profileStops.map { NSNumber(value: Double($0.location)) }
        boostLayer.locations = baseLayer.locations

        let featherStops = DockletGlowProfile.featherStops(length: bounds.height)
        featherLayer.colors = featherStops.map { UIColor.black.withAlphaComponent($0.alpha).cgColor }
        featherLayer.locations = featherStops.map { NSNumber(value: Double($0.location)) }
    }

    /// Scales the band from the screen edge. A hold springs out, a release eases back, and an unanimated change lands
    /// at once.
    func setBandScale(_ scale: CGFloat, animated: Bool, spring: Bool) {
        if animated {
            bandLayer.add(scaleAnimation(to: scale, spring: spring), forKey: Self.scaleKey)
        } else {
            bandLayer.removeAnimation(forKey: Self.scaleKey)
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        bandLayer.transform = CATransform3DMakeScale(scale, 1, 1)
        CATransaction.commit()
    }

    func scaleAnimation(to scale: CGFloat, spring: Bool) -> CAAnimation {
        guard spring else {
            let release = CABasicAnimation(keyPath: "transform.scale.x")
            release.toValue = scale
            release.duration = Self.fadeDuration
            release.timingFunction = CAMediaTimingFunction(name: .easeOut)
            return release
        }
        let overshoot = CASpringAnimation(keyPath: "transform.scale.x")
        overshoot.toValue = scale
        overshoot.mass = 1
        overshoot.stiffness = Self.springStiffness
        overshoot.damping = 2 * Self.springDampingRatio * Self.springStiffness.squareRoot()
        overshoot.duration = Self.heldScaleDuration
        return overshoot
    }

    /// Moves the shimmer one pass of its colours along the glow. The layer is twice the glow's height, so the pass
    /// restarts without a gap.
    func startShimmerFlow() {
        let flow = CABasicAnimation(keyPath: "transform.translation.y")
        flow.fromValue = 0
        flow.toValue = -bounds.height
        flow.duration = Self.shimmerPeriod
        flow.timingFunction = CAMediaTimingFunction(name: .linear)
        flow.repeatCount = .infinity
        shimmerLayer.add(flow, forKey: Self.shimmerKey)
    }

    /// The release fade ends with the shimmer's movement removed, unless a new hold has started meanwhile.
    func stopShimmerFlowIfReleased() {
        guard !isHeld else {
            return
        }
        shimmerLayer.removeAnimation(forKey: Self.shimmerKey)
    }
}
