import Foundation

struct RepositoryScorecardWorkflowWriteResult: Equatable, Sendable {
    let relativePath: String
    let absolutePath: String
    let created: Bool
}

enum RepositoryScorecardWorkflowWriter {
    static let relativePath = ".github/workflows/scorecard.yml"

    static func write(
        in repositoryRoot: String,
        fileManager: FileManager = .default
    ) throws -> RepositoryScorecardWorkflowWriteResult {
        let rootURL = URL(fileURLWithPath: repositoryRoot, isDirectory: true)
            .standardizedFileURL
        var isDirectory = ObjCBool(false)
        guard fileManager.fileExists(atPath: rootURL.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw RepositoryScorecardWorkflowWriterError.repositoryMissing(repositoryRoot)
        }

        let workflowURL = rootURL.appendingPathComponent(relativePath, isDirectory: false)
        let workflowDirectoryURL = workflowURL.deletingLastPathComponent()

        if fileManager.fileExists(atPath: workflowURL.path, isDirectory: &isDirectory) {
            guard !isDirectory.boolValue else {
                throw RepositoryScorecardWorkflowWriterError.workflowPathIsDirectory(workflowURL.path)
            }
            return RepositoryScorecardWorkflowWriteResult(
                relativePath: relativePath,
                absolutePath: workflowURL.path,
                created: false
            )
        }

        if fileManager.fileExists(atPath: workflowDirectoryURL.path, isDirectory: &isDirectory),
           !isDirectory.boolValue {
            throw RepositoryScorecardWorkflowWriterError.workflowDirectoryIsFile(workflowDirectoryURL.path)
        }

        try fileManager.createDirectory(
            at: workflowDirectoryURL,
            withIntermediateDirectories: true
        )
        try workflowContents.write(to: workflowURL, atomically: true, encoding: .utf8)
        return RepositoryScorecardWorkflowWriteResult(
            relativePath: relativePath,
            absolutePath: workflowURL.path,
            created: true
        )
    }

    static let workflowContents = """
    name: Scorecard

    on:
      workflow_dispatch:
      schedule:
        - cron: "27 4 * * 1"

    permissions: {}

    jobs:
      scorecard:
        name: OpenSSF Scorecard
        runs-on: ubuntu-latest
        permissions:
          contents: read
          actions: read
          security-events: write
          id-token: write
        steps:
          - name: Checkout repository
            uses: actions/checkout@v4
            with:
              persist-credentials: false

          - name: Run Scorecard
            uses: ossf/scorecard-action@v2.4.0
            with:
              results_format: sarif
              results_file: scorecard.sarif
              publish_results: true

          - name: Upload SARIF
            uses: github/codeql-action/upload-sarif@v3
            with:
              sarif_file: scorecard.sarif
    """
}

enum RepositoryScorecardWorkflowWriterError: LocalizedError, Equatable {
    case repositoryMissing(String)
    case workflowDirectoryIsFile(String)
    case workflowPathIsDirectory(String)

    var errorDescription: String? {
        switch self {
        case let .repositoryMissing(path):
            return "Scorecard workflow could not be written because the repository root does not exist at \(path)."
        case let .workflowDirectoryIsFile(path):
            return "Scorecard workflow could not be written because \(path) is a file, not a directory."
        case let .workflowPathIsDirectory(path):
            return "Scorecard workflow could not be written because \(path) is a directory."
        }
    }
}
