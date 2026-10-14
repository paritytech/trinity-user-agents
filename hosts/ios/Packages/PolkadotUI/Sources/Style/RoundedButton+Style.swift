import UIKit
public import UIKit_iOS
import DesignSystem

public extension RoundedButton {
    func applyBaseFillStyle() {
        roundedBackgroundView?.shadowOpacity = 0.0
        roundedBackgroundView?.strokeColor = .clear
        roundedBackgroundView?.highlightedStrokeColor = .clear
        roundedBackgroundView?.cornerRadius = 12
        roundedBackgroundView?.enableDynamicColorReapply()

        changesContentOpacityWhenHighlighted = true
    }

    func applyBaseMainStyle() {
        applyBaseFillStyle()

        imageWithTitleView?.titleColor = .fgPrimaryInverted
        imageWithTitleView?.titleFont = UIFont.titleMedium
    }

    func applyMainStyle() {
        applyBaseMainStyle()

        roundedBackgroundView?.fillColor = .bgActionPrimary
        roundedBackgroundView?.highlightedFillColor = .bgActionPrimary
    }

    func applyBaseSecondaryStyle() {
        applyBaseFillStyle()

        imageWithTitleView?.titleColor = .fgPrimary
        imageWithTitleView?.titleFont = UIFont.titleMedium
    }

    func applySecondaryStyle(titleFont: UIFont = UIFont.titleMedium) {
        applyBaseSecondaryStyle()

        imageWithTitleView?.titleFont = titleFont
        roundedBackgroundView?.fillColor = .bgActionTertiary
        roundedBackgroundView?.highlightedFillColor = .bgActionTertiary
    }

    func applyTitleSecondaryStyle() {
        applyTitleStyle()

        imageWithTitleView?.titleColor = .fgSecondary
        imageWithTitleView?.titleFont = UIFont.titleMedium
        imageWithTitleView?.spacingBetweenLabelAndIcon = 0.0
    }

    func applyTitleTertiaryStyle() {
        applyTitleStyle()

        imageWithTitleView?.titleColor = .fgTertiary
        imageWithTitleView?.titleFont = UIFont.titleMedium
        imageWithTitleView?.spacingBetweenLabelAndIcon = 0.0
    }

    func applyTitleStyle() {
        roundedBackgroundView?.shadowOpacity = 0.0
        roundedBackgroundView?.fillColor = .clear
        roundedBackgroundView?.highlightedFillColor = .clear
        roundedBackgroundView?.strokeColor = .clear
        roundedBackgroundView?.highlightedStrokeColor = .clear

        changesContentOpacityWhenHighlighted = true

        imageWithTitleView?.titleColor = .fgTertiary
        imageWithTitleView?.titleFont = UIFont.titleMedium

        imageWithTitleView?.spacingBetweenLabelAndIcon = 0.0
    }

    func applyIconStyle() {
        roundedBackgroundView?.shadowOpacity = 0.0
        roundedBackgroundView?.fillColor = .clear
        roundedBackgroundView?.highlightedFillColor = .clear
        roundedBackgroundView?.strokeColor = .clear
        roundedBackgroundView?.highlightedStrokeColor = .clear

        changesContentOpacityWhenHighlighted = true

        imageWithTitleView?.spacingBetweenLabelAndIcon = 0.0
    }

    func applyTitleIconStyle() {
        applyTitleStyle()

        imageWithTitleView?.spacingBetweenLabelAndIcon = 8.0
    }

    func applyCaptionTitleIconStyle() {
        roundedBackgroundView?.shadowOpacity = 0.0
        roundedBackgroundView?.strokeColor = .clear
        roundedBackgroundView?.highlightedStrokeColor = .clear
        roundedBackgroundView?.cornerRadius = 32
        roundedBackgroundView?.fillColor = .bgActionTertiary
        imageWithTitleView?.titleColor = .fgPrimary
        imageWithTitleView?.titleFont = UIFont.paragraphSmall
        contentInsets = .init(top: 0, left: 0, bottom: 0, right: 16)
        changesContentOpacityWhenHighlighted = true
    }

    func applyDestructiveStyle() {
        applyBaseSecondaryStyle()

        imageWithTitleView?.titleColor = .fgStaticWhite
        roundedBackgroundView?.fillColor = .bgStatusError
        roundedBackgroundView?.highlightedFillColor = .bgStatusError
    }

    func applyDisabledStyle() {
        applyBaseSecondaryStyle()

        imageWithTitleView?.titleColor = .fgDisabled
        roundedBackgroundView?.fillColor = .bgActionTertiary
        roundedBackgroundView?.highlightedFillColor = .bgActionTertiary
    }

    func applyBarButtonItemStyle() {
        applyBaseFillStyle()

        roundedBackgroundView?.fillColor = .bgActionTertiary
        roundedBackgroundView?.highlightedFillColor = .bgActionTertiary
        contentInsets = UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)
        roundedBackgroundView?.cornerRadius = 20

        changesContentOpacityWhenHighlighted = true
    }
}
