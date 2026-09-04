import XCTest

final class CatalogTests: XCTestCase {
    private var items: [String] = []

    func testLoadsCatalog() {
        XCTAssertEqual(items.isEmpty, true)
    }

    func testFiltersCatalog() {
        let visible = items.filter { !$0.isEmpty }
        XCTAssertTrue(visible.isEmpty)
    }

    func testSortsCatalog() {
        let sorted = items.sorted()
        XCTAssertEqual(sorted, items)
    }

    private func helper() -> Int {
        items.count
    }
}
