#if DEBUG
    import DesignSystem
    import PolkadotUI
    import SwiftUI

    /// A circle with a cursor in it, for setting a direction by hand: the unit disc, screen y
    /// downward, centre meaning none and the rim meaning as far as the thing it drives can go.
    struct CoinageDial: View {
        @Binding var cursor: CGPoint
        let title: String

        private static let diameter: CGFloat = 120
        private static let knob: CGFloat = 16

        var body: some View {
            VStack(spacing: DSSpacings.extraSmall) {
                Text(verbatim: title)
                    .typography(.bodySmall)
                    .foregroundStyle(Color.fgSecondary)
                pad
            }
        }

        private var pad: some View {
            let radius = Self.diameter / 2

            return ZStack {
                Circle()
                    .stroke(Color.fgSecondary, lineWidth: 1)
                Path { path in
                    path.move(to: CGPoint(x: radius, y: 0))
                    path.addLine(to: CGPoint(x: radius, y: Self.diameter))
                    path.move(to: CGPoint(x: 0, y: radius))
                    path.addLine(to: CGPoint(x: Self.diameter, y: radius))
                }
                .stroke(Color.fgSecondary.opacity(0.4), lineWidth: 1)
                Circle()
                    .fill(Color.fgPrimary)
                    .frame(width: Self.knob, height: Self.knob)
                    .offset(x: cursor.x * radius, y: cursor.y * radius)
            }
            .frame(width: Self.diameter, height: Self.diameter)
            .contentShape(Circle())
            .highPriorityGesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in
                        cursor = Self.clamped(
                            CGPoint(
                                x: (value.location.x - radius) / radius,
                                y: (value.location.y - radius) / radius
                            )
                        )
                    }
            )
        }

        private static func clamped(_ point: CGPoint) -> CGPoint {
            let length = hypot(point.x, point.y)

            return length > 1 ? CGPoint(x: point.x / length, y: point.y / length) : point
        }
    }

    /// The light and the coins, each held by hand while the toggle is on, for looking at the
    /// depiction from a chosen angle on a device that has no motion or a hand that cannot hold
    /// one. The light dial stands in for the phone pose, a right angle at the rim; the coin dial
    /// leans every coin toward the cursor's side, edge on at the rim. Debug builds only; the
    /// shipped coins follow the phone and never lean.
    struct CoinageLightPad: View {
        @Binding var turn: CoinageTilt.Turn?
        @Binding var lean: CoinageTilt.Turn?

        @State private var lightCursor: CGPoint = .zero
        @State private var coinCursor: CGPoint = .zero

        var body: some View {
            VStack(alignment: .leading, spacing: DSSpacings.small) {
                Toggle(isOn: isFixed) {
                    Text(verbatim: "Fixed light")
                        .typography(.bodySmall)
                        .foregroundStyle(Color.fgPrimary)
                }

                if turn != nil {
                    HStack(spacing: DSSpacings.large) {
                        CoinageDial(cursor: $lightCursor, title: "Light")
                            .onChange(of: lightCursor) { _, cursor in turn = Self.turn(for: cursor) }

                        CoinageDial(cursor: $coinCursor, title: "Coins")
                            .onChange(of: coinCursor) { _, cursor in lean = Self.lean(for: cursor) }
                    }
                    .frame(maxWidth: .infinity)
                }
            }
        }

        private var isFixed: Binding<Bool> {
            Binding(
                get: { turn != nil },
                set: { fixed in
                    turn = fixed ? Self.turn(for: lightCursor) : nil
                    lean = fixed ? Self.lean(for: coinCursor) : nil
                }
            )
        }

        static func turn(for cursor: CGPoint) -> CoinageTilt.Turn {
            CoinageTilt.studioTurn(lean: -Double(cursor.y) * .pi / 2, sideways: Double(cursor.x) * .pi / 2)
        }

        static func lean(for cursor: CGPoint) -> CoinageTilt.Turn {
            CoinageTilt.leaned(pitch: Double(cursor.y) * .pi / 2, yaw: Double(cursor.x) * .pi / 2)
        }
    }
#endif
