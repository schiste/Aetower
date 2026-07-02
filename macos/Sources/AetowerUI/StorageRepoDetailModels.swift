import Foundation

/// Decode model for the per-repository storage drill-down
/// (`storage_hygiene_repo_detail_json`). Composed from the existing hygiene
/// component models; decoding stays lenient (`decodeIfPresent` + defaults) so
/// later payload enrichments flow through without breaking older responses.
struct StorageRepoDetailModel: Decodable, Sendable {
    let capturedAtMillis: UInt64
    let scanMode: String
    let repository: StorageRepositoryInventoryModel?
    let repoFootprints: [StorageRepoFootprintModel]
    let items: [StorageHygieneItemModel]
    let cleanupRecipes: [StorageCleanupRecipeModel]
    let cleanupBundles: [StorageCleanupBundleModel]
    let caveats: [String]

    enum CodingKeys: String, CodingKey {
        case capturedAtMillis
        case scanMode
        case repository
        case repoFootprints
        case items
        case cleanupRecipes
        case cleanupBundles
        case caveats
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        capturedAtMillis = try container.decodeIfPresent(UInt64.self, forKey: .capturedAtMillis) ?? 0
        scanMode = try container.decodeIfPresent(String.self, forKey: .scanMode) ?? "instant_cached"
        repository = try container.decodeIfPresent(StorageRepositoryInventoryModel.self, forKey: .repository)
        repoFootprints = try container.decodeIfPresent([StorageRepoFootprintModel].self, forKey: .repoFootprints) ?? []
        items = try container.decodeIfPresent([StorageHygieneItemModel].self, forKey: .items) ?? []
        cleanupRecipes = try container.decodeIfPresent([StorageCleanupRecipeModel].self, forKey: .cleanupRecipes) ?? []
        cleanupBundles = try container.decodeIfPresent([StorageCleanupBundleModel].self, forKey: .cleanupBundles) ?? []
        caveats = try container.decodeIfPresent([String].self, forKey: .caveats) ?? []
    }
}
