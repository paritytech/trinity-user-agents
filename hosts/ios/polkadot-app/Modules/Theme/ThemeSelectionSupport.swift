import DesignSystem
import SwiftUI
import UIKit

// MARK: - Theme Selection

extension ThemeSelection {
    var displayName: String {
        var result = ""
        for scalar in rawValue.unicodeScalars {
            if CharacterSet.uppercaseLetters.contains(scalar), !result.isEmpty {
                result.unicodeScalars.append(" ")
            }
            result.unicodeScalars.append(scalar)
        }
        return result.capitalized
    }
}

// MARK: - Entrance

enum ThemeSelectionMetrics {
    static let bubbleInset: CGFloat = 64
    static let swatchSize: CGFloat = 48
    static let swatchDotSize: CGFloat = 24
    // Floor for the sheet's off-screen start, used only until the container height is measured
    // (avoids a first-frame flash). The measured container height drives the actual travel.
    static let sheetOffsetFloor: CGFloat = 600
}

enum ThemeSelectionEntrance {
    static let bubbleCount = 3
    static let bubbleStagger = 0.15
    static let bubbleDuration = 0.35
    static let bubbleTravel: CGFloat = 40

    static let swatchStagger = 0.06
    static let swatchPop = Animation.spring(response: 0.35, dampingFraction: 0.6)
}

struct BubbleEntrance: ViewModifier {
    let appeared: Bool
    let index: Int

    func body(content: Content) -> some View {
        content
            .geometryGroup()
            .opacity(appeared ? 1 : 0)
            .offset(y: appeared ? 0 : ThemeSelectionEntrance.bubbleTravel)
            .animation(
                .spring(duration: ThemeSelectionEntrance.bubbleDuration)
                    .delay(Double(index) * ThemeSelectionEntrance.bubbleStagger),
                value: appeared
            )
    }
}

// MARK: - Theme Reveal

struct RevealDescriptor: Identifiable {
    let id: Int
    let selection: ThemeSelection
    let center: CGPoint
    let targetDiameter: CGFloat
}

struct RevealQueue {
    private(set) var descriptors: [RevealDescriptor] = []
    private var generation = 0

    mutating func add(selection: ThemeSelection, center: CGPoint, targetDiameter: CGFloat) {
        generation += 1
        descriptors.append(
            RevealDescriptor(id: generation, selection: selection, center: center, targetDiameter: targetDiameter)
        )
    }

    mutating func removeUpThrough(id: Int) {
        descriptors.removeAll { $0.id <= id }
    }
}

struct ThemeRevealLayer<Content: View>: View {
    let center: CGPoint
    let targetDiameter: CGFloat
    let duration: Double
    let onCovered: () -> Void
    let content: Content

    @State private var diameter: CGFloat

    init(
        center: CGPoint,
        startDiameter: CGFloat,
        targetDiameter: CGFloat,
        duration: Double,
        onCovered: @escaping () -> Void,
        @ViewBuilder content: () -> Content
    ) {
        self.center = center
        self.targetDiameter = targetDiameter
        self.duration = duration
        self.onCovered = onCovered
        self.content = content()
        _diameter = State(initialValue: startDiameter)
    }

    var body: some View {
        content
            .mask {
                Circle()
                    .frame(width: diameter, height: diameter)
                    .position(center)
            }
            .onAppear {
                withAnimation(.easeInOut(duration: duration)) {
                    diameter = targetDiameter
                } completion: {
                    onCovered()
                }
            }
    }
}

struct SwatchButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.88 : 1)
            .animation(.spring(response: 0.3, dampingFraction: 0.55), value: configuration.isPressed)
    }
}

struct SwatchCenterPreferenceKey: PreferenceKey {
    static let defaultValue: [ThemeSelection: CGPoint] = [:]

    static func reduce(value: inout [ThemeSelection: CGPoint], nextValue: () -> [ThemeSelection: CGPoint]) {
        value.merge(nextValue()) { _, new in new }
    }
}

extension CGRect {
    var center: CGPoint { CGPoint(x: midX, y: midY) }
}
