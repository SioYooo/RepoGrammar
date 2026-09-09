import XCTest

final class DegradedTests: XCTestCase {
    @available(iOS 15, *)
    func testRequiresModernOS() {
        // An attribute with a non-empty argument list is outside the
        // declared subset, so the whole file abstains.
    }

    func testWouldOtherwiseAnchor() {
        // The hostile construct above refuses the file this method sits in.
    }
}
