import UIKit
public import UIKit_iOS

extension RoundedView {
    public func applyBackgroundStyle(with cornerRadius: CGFloat) {
        shadowOpacity = 0
        self.cornerRadius = cornerRadius
    }

    /// Text And Icons Primary Dark / 12pt
    public func applyPrimaryMedium() {
        applyBackgroundStyle(UIColor(resource: .textAndIconsPrimaryDark), cornerRadius: 12)
    }

    public func applyClear() {
        applyBackgroundStyle(.clear, cornerRadius: 0)
        applyBorderStyle(.clear, cornerRadius: 0)
    }

    /// color141414 / 48pt
    func applyCapsule141414() {
        applyBackgroundStyle(UIColor(resource: .color141414), cornerRadius: 48)
    }

    func applyCapsuleFill6() {
        applyBackgroundStyle(UIColor(resource: .fill6), cornerRadius: 32)
    }

    /// Background Secondary / 16pt
    func applyRounded16Secondary() {
        applyBackgroundStyle(UIColor(resource: .backgroundSecondary), cornerRadius: 16)
    }

    func applyRoundedFill6() {
        applyBackgroundStyle(UIColor(resource: .fill6), cornerRadius: 16)
    }

    /// Fill12 / 12pt
    func applyRoundedFill12() {
        applyBackgroundStyle(UIColor(resource: .fill12), cornerRadius: 12)
    }

    func applyLargeRoundedFill6() {
        applyBackgroundStyle(UIColor(resource: .fill6), cornerRadius: 24)
    }

    func applySlightlyRoundedFill6() {
        applyBackgroundStyle(UIColor(resource: .fill6), cornerRadius: 8)
    }

    func applyRoundedLight() {
        applyBackgroundStyle(UIColor(resource: .white100), cornerRadius: 24)
    }

    /// White 100 / 32pt
    func applyRoundedLargeLight() {
        applyBackgroundStyle(UIColor(resource: .white100), cornerRadius: 32)
    }

    /// Background Tertiary / 6pt
    func applyRoundedTertiary6() {
        applyBackgroundStyle(UIColor(resource: .backgroundTertiary), cornerRadius: 6)
    }

    /// Background Tertiary / 16pt
    func applyRoundedTertiary() {
        applyBackgroundStyle(UIColor(resource: .backgroundTertiary), cornerRadius: 16)
    }

    /// Background Secondary / 32pt
    func applyRoundedSecondary() {
        applyBackgroundStyle(UIColor(resource: .backgroundSecondary), cornerRadius: 32)
    }

    func applyRoundedEmptyBorder() {
        applyBackgroundStyle(.clear, cornerRadius: 24)
        strokeColor = UIColor(resource: .appliedStroke)
        strokeWidth = 1
    }

    func applyFill6Circle(_ radius: CGFloat) {
        applyBackgroundStyle(UIColor(resource: .fill6), cornerRadius: radius)
        snp.makeConstraints { make in
            make.height.equalTo(snp.width)
            make.width.equalTo(radius)
        }
    }

    func applyFill12Circle(_ radius: CGFloat) {
        applyBackgroundStyle(UIColor(resource: .fill12), cornerRadius: radius)
        snp.makeConstraints { make in
            make.height.equalTo(snp.width)
            make.width.equalTo(radius)
        }
    }

    func applyFill18Circle(_ radius: CGFloat) {
        applyBackgroundStyle(UIColor(resource: .fill18), cornerRadius: radius)
        snp.makeConstraints { make in
            make.height.equalTo(snp.width)
            make.width.equalTo(radius)
        }
    }

    func applyFill30Rounded() {
        applyBackgroundStyle(UIColor(resource: .fill30), cornerRadius: 16)
    }

    func applyBlackCircle(_ radius: CGFloat) {
        applyBackgroundStyle(UIColor(resource: .black100), cornerRadius: radius)
        snp.makeConstraints { make in
            make.height.equalTo(snp.width)
            make.width.equalTo(radius)
        }
    }

    /// Black100 / 8pt
    func applyBlackIcon() {
        applyBackgroundStyle(UIColor(resource: .black100), cornerRadius: 8)
    }
}
