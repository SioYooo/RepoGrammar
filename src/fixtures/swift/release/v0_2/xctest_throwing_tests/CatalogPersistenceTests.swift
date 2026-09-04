import XCTest

enum CatalogError: Error {
    case empty
}

struct Record: Equatable {
    let name: String
}

final class CatalogPersistenceTests: XCTestCase {
    private var records: [Record] = []

    override func setUp() throws {
        try super.setUp()
        records = [Record(name: "alpha")]
    }

    func testLoadReturnsStoredRecords() throws {
        let loaded = try loadRecords()
        XCTAssertEqual(loaded.count, 1)
    }

    func testLoadThrowsWhenEmpty() throws {
        records = []
        XCTAssertThrowsError(try loadRecords())
    }

    func testAppendPersistsNewRecord() throws {
        try append(Record(name: "beta"))
        XCTAssertEqual(records.count, 2)
    }

    private func loadRecords() throws -> [Record] {
        if records.isEmpty {
            throw CatalogError.empty
        }
        return records
    }

    private func append(_ record: Record) throws {
        records.append(record)
    }
}
