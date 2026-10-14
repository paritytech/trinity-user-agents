import SwiftUI
import DesignSystem

public struct CountdownTimerView: View {
    private let totalTime: TimeInterval
    private let size: CGSize
    private let arcLength: CGFloat

    @State private var remainingTime: TimeInterval
    @State private var isSpinning = false

    private let timer = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    public init(
        totalTime: TimeInterval,
        size: CGSize,
        arcLength: CGFloat = 0.25
    ) {
        self.totalTime = totalTime
        self.size = size
        self.arcLength = arcLength
        _remainingTime = State(initialValue: max(0, totalTime))
    }

    public var body: some View {
        ZStack {
            Circle()
                .stroke(lineWidth: 4)
                .foregroundStyle(Color(.white6))

            Circle()
                .trim(from: 0, to: arcLength)
                .stroke(
                    style: StrokeStyle(
                        lineWidth: 4,
                        lineCap: .round,
                        lineJoin: .round
                    )
                )
                .foregroundStyle(Color(.white100))
                .rotationEffect(.degrees(isSpinning ? 360 : 0))
                .animation(
                    .easeInOut(duration: 1.0)
                        .repeatForever(autoreverses: false),
                    value: isSpinning
                )

            Text(.countdownSec(Int32(ceil(remainingTime))))
                .typography(.paragraphSmall)
                .monospacedDigit()
                .foregroundStyle(Color(.white100))
                .contentTransition(.numericText(countsDown: true))
        }
        .frame(width: size.width, height: size.height)
        .onAppear {
            isSpinning = true
        }
        .onChange(of: totalTime) { _, newValue in
            remainingTime = max(0, newValue)
            isSpinning = true
        }
        .onReceive(timer) { _ in
            guard remainingTime > 0 else {
                return
            }

            withAnimation {
                remainingTime -= 1
            }
        }
    }
}
