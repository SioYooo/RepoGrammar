import XCTest

#if DEBUG
final class DebugOnlyTests: XCTestCase {
    func testDebugOnly() {
        // Compiled only under the DEBUG build configuration, which no bytes
        // in this file decide.
    }
}
#endif

final class AlwaysTests: XCTestCase {
    func testAlways() {
        // The conditional region above still makes the whole file abstain:
        // nothing proves which declarations the build contains.
    }
}
