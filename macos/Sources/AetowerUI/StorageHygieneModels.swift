import Foundation

struct StorageHygieneReportModel: Decodable {
    let capturedAtMillis: UInt64
    let scanDurationMillis: UInt64
    let summary: StorageHygieneSummaryModel
    let items: [StorageHygieneItemModel]
    let roots: [String]
    let skippedRoots: [StorageSkippedRootModel]
    let truncated: Bool
    let caveats: [String]
}

struct StorageHygieneSummaryModel: Decodable {
    let itemCount: Int
    let totalReclaimableBytes: UInt64
    let safeCandidateCount: Int
    let reviewCandidateCount: Int
    let staleCandidateCount: Int
    let scannedDirectoryCount: UInt64
    let largestItemPath: String?
    let largestItemBytes: UInt64
}

struct StorageHygieneItemModel: Decodable, Identifiable {
    let id: String
    let path: String
    let displayName: String
    let kind: String
    let safety: String
    let sizeBytes: UInt64
    let sizeTruncated: Bool
    let modifiedMillis: UInt64?
    let ageDays: UInt64?
    let stale: Bool
    let reason: String
    let recommendation: String
    let commandHint: String
}

struct StorageSkippedRootModel: Decodable, Identifiable {
    let path: String
    let reason: String

    var id: String { "\(path)|\(reason)" }
}
