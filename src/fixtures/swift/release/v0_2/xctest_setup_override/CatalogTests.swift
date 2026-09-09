import XCTest

final class CatalogSetupTests: XCTestCase {
    private var catalog: [String: Int] = [:]

    override func setUp() {
        super.setUp()
        catalog = ["alpha": 1]
    }

    override func tearDown() {
        catalog.removeAll()
        super.tearDown()
    }

    func testCatalogIsReady() {
        XCTAssertEqual(catalog["alpha"], 1)
    }

    func testCatalogHasOneEntry() {
        XCTAssertEqual(catalog.count, 1)
    }
}
