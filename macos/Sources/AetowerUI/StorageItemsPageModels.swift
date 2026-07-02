import Foundation

/// Decode model for the server-paged Storage Explorer items endpoint
/// (`storage_hygiene_items_page_json`). Mirrors the Rust
/// `StorageHygieneItemsPageResponse` envelope. Decoding stays lenient
/// (`decodeIfPresent` + defaults) so older engines and later payload
/// enrichments both decode without breaking; decode with
/// `AetowerJSON.snakeCaseDecoder()`.
struct StorageHygieneItemsPageModel: Decodable, Sendable {
    let capturedAtMillis: UInt64
    let scanMode: String
    let offset: Int
    let limit: Int
    let sortKey: String
    let sortDescending: Bool
    let returnedCount: Int
    /// Total rows the server can page through for this sort; nil on older
    /// engines that do not report it, in which case the UI hides "of Z".
    let totalAvailable: Int?
    let hasMore: Bool
    let items: [StorageHygieneItemModel]
    let diagnostics: StorageScanDiagnosticsModel?

    private enum CodingKeys: String, CodingKey {
        case capturedAtMillis
        case scanMode
        case offset
        case limit
        case sortKey
        case sortDescending
        case returnedCount
        case totalAvailable
        case hasMore
        case items
        case diagnostics
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        capturedAtMillis = try container.decodeIfPresent(UInt64.self, forKey: .capturedAtMillis) ?? 0
        scanMode = try container.decodeIfPresent(String.self, forKey: .scanMode) ?? "instant_cached"
        items = try container.decodeIfPresent([StorageHygieneItemModel].self, forKey: .items) ?? []
        offset = try container.decodeIfPresent(Int.self, forKey: .offset) ?? 0
        limit = try container.decodeIfPresent(Int.self, forKey: .limit) ?? items.count
        sortKey = try container.decodeIfPresent(String.self, forKey: .sortKey) ?? "size"
        sortDescending = try container.decodeIfPresent(Bool.self, forKey: .sortDescending) ?? true
        returnedCount = try container.decodeIfPresent(Int.self, forKey: .returnedCount) ?? items.count
        let total = try container.decodeIfPresent(Int.self, forKey: .totalAvailable)
        totalAvailable = total
        if let hasMore = try container.decodeIfPresent(Bool.self, forKey: .hasMore) {
            self.hasMore = hasMore
        } else if let total {
            self.hasMore = offset + items.count < total
        } else {
            self.hasMore = !items.isEmpty && items.count >= limit
        }
        // Diagnostics are informational only; a shape drift there must not
        // sink the whole page, so tolerate decode failures as absence.
        diagnostics = (try? container.decodeIfPresent(StorageScanDiagnosticsModel.self, forKey: .diagnostics)) ?? nil
    }
}
