import PolkadotUI
import XCTest

final class TextDiffCalculatorTests: XCTestCase {
    private var calculator: TextDiffCalculator!

    override func setUp() {
        super.setUp()
        calculator = TextDiffCalculator()
    }

    override func tearDown() {
        calculator = nil
        super.tearDown()
    }

    // MARK: - Basic Tests

    func testIdenticalTexts() {
        let result = calculator.computeDiff(from: "Hello World", to: "Hello World")

        XCTAssertEqual(result.count, 1)
        XCTAssertEqual(result[0], .unchanged("Hello World"))
    }

    func testEmptyToText() {
        let result = calculator.computeDiff(from: "", to: "Hello")

        XCTAssertEqual(result.count, 1)
        XCTAssertEqual(result[0], .added("Hello"))
    }

    func testTextToEmpty() {
        let result = calculator.computeDiff(from: "Hello", to: "")

        XCTAssertEqual(result.count, 1)
        XCTAssertEqual(result[0], .deleted("Hello"))
    }

    func testBothEmpty() {
        let result = calculator.computeDiff(from: "", to: "")

        XCTAssertTrue(result.isEmpty)
    }

    // MARK: - Word Addition Tests

    func testAddWordAtEnd() {
        let result = calculator.computeDiff(from: "Hello", to: "Hello World")

        XCTAssertEqual(result.count, 2)
        XCTAssertEqual(result[0], .unchanged("Hello"))
        XCTAssertEqual(result[1], .added(" World"))
    }

    func testAddWordAtBeginning() {
        let result = calculator.computeDiff(from: "World", to: "Hello World")

        XCTAssertEqual(result.count, 2)
        XCTAssertEqual(result[0], .added("Hello "))
        XCTAssertEqual(result[1], .unchanged("World"))
    }

    func testAddWordInMiddle() {
        let result = calculator.computeDiff(from: "Hello World", to: "Hello Beautiful World")

        XCTAssertEqual(result.count, 3)
        XCTAssertEqual(result[0], .unchanged("Hello "))
        XCTAssertEqual(result[1], .added("Beautiful "))
        XCTAssertEqual(result[2], .unchanged("World"))
    }

    // MARK: - Word Deletion Tests

    func testDeleteWordAtEnd() {
        let result = calculator.computeDiff(from: "Hello World", to: "Hello")

        XCTAssertEqual(result.count, 2)
        XCTAssertEqual(result[0], .unchanged("Hello"))
        XCTAssertEqual(result[1], .deleted(" World"))
    }

    func testDeleteWordAtBeginning() {
        let result = calculator.computeDiff(from: "Hello World", to: "World")

        XCTAssertEqual(result.count, 2)
        XCTAssertEqual(result[0], .deleted("Hello "))
        XCTAssertEqual(result[1], .unchanged("World"))
    }

    func testDeleteWordInMiddle() {
        let result = calculator.computeDiff(from: "Hello Beautiful World", to: "Hello World")

        XCTAssertEqual(result.count, 3)
        XCTAssertEqual(result[0], .unchanged("Hello "))
        XCTAssertEqual(result[1], .deleted("Beautiful "))
        XCTAssertEqual(result[2], .unchanged("World"))
    }

    // MARK: - Word Replacement Tests

    func testReplaceWord() {
        let result = calculator.computeDiff(from: "Hello World", to: "Hello Universe")

        XCTAssertEqual(result.count, 3)
        XCTAssertEqual(result[0], .unchanged("Hello "))
        XCTAssertEqual(result[1], .deleted("World"))
        XCTAssertEqual(result[2], .added("Universe"))
    }

    func testReplaceMultipleWords() {
        let result = calculator.computeDiff(from: "The quick fox", to: "The slow dog")

        XCTAssertTrue(result.contains(.unchanged("The ")))
        XCTAssertTrue(result.contains(.deleted("quick")))
        XCTAssertTrue(result.contains(.added("slow")))
        XCTAssertTrue(result.contains(.deleted("fox")))
        XCTAssertTrue(result.contains(.added("dog")))
    }

    // MARK: - Punctuation Tests

    func testPunctuationPreserved() {
        let result = calculator.computeDiff(from: "Hello, World!", to: "Hello, World!")

        XCTAssertEqual(result.count, 1)
        XCTAssertEqual(result[0], .unchanged("Hello, World!"))
    }

    func testPunctuationChanged() {
        let result = calculator.computeDiff(from: "Hello, World!", to: "Hello. World?")

        XCTAssertTrue(result.contains(.unchanged("Hello")))
        XCTAssertTrue(result.contains(.deleted(",")))
        XCTAssertTrue(result.contains(.added(".")))
        XCTAssertTrue(result.contains(.unchanged(" World")))
        XCTAssertTrue(result.contains(.deleted("!")))
        XCTAssertTrue(result.contains(.added("?")))
    }

    // MARK: - Complex Tests

    func testComplexDiff() {
        let oldText = "I went to the store yesterday"
        let newText = "I went to the market today"

        let result = calculator.computeDiff(from: oldText, to: newText)

        // Should have unchanged "I went to the ", deleted "store", added "market",
        // unchanged " ", deleted "yesterday", added "today"
        XCTAssertTrue(result.contains(.unchanged("I went to the ")))
        XCTAssertTrue(result.contains(.deleted("store")))
        XCTAssertTrue(result.contains(.added("market")))
        XCTAssertTrue(result.contains(.deleted("yesterday")))
        XCTAssertTrue(result.contains(.added("today")))
    }

    func testWhitespaceHandling() {
        let result = calculator.computeDiff(from: "Hello   World", to: "Hello World")

        // The algorithm treats each space as a separate token
        XCTAssertTrue(result.contains(.unchanged("Hello ")))
        XCTAssertTrue(result.contains(.unchanged("World")))
    }

    // MARK: - Merge Tests

    func testConsecutiveAdditionsMerged() {
        let result = calculator.computeDiff(from: "A", to: "A B C")

        // "B C" should be merged into a single added part
        let addedParts = result.filter {
            if case .added = $0 { return true }
            return false
        }
        XCTAssertEqual(addedParts.count, 1)
    }

    func testConsecutiveDeletionsMerged() {
        let result = calculator.computeDiff(from: "A B C", to: "A")

        // " B C" should be merged into a single deleted part
        let deletedParts = result.filter {
            if case .deleted = $0 { return true }
            return false
        }
        XCTAssertEqual(deletedParts.count, 1)
    }
}
