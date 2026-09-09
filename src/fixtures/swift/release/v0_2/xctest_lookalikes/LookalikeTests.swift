import XCTest

class CatalogHelper {
    let label = "helper"

    func testLooksLikeATest() {
        // Legal Swift, but the enclosing class does not derive from
        // XCTestCase, so XCTest never discovers this method.
    }
}

final class LookalikeTests: XCTestCase {
    static func testStaticLookalike() {
        // XCTest discovers only instance methods.
    }

    func testWithParameter(value: Int) {
        // An XCTest test method takes no parameters.
    }
}

func testFreeStanding() {
    // A free function is never discovered by XCTest, however it is spelled.
}
