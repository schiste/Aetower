#!/bin/sh
# Sourced by ci-local.sh; run/skip and run_swift_tests share its gate labels.
run_full_swift_checks() {
    if [ -z "$INCLUDE_FILTER" ] && [ -z "$SKIP_FILTER" ]; then
        # SwiftPM test builds every product, including the app executable.
        # Avoid a separate package plan/plugin pass before that same build.
        run_swift_tests
        skip "swift build" "all products built by swift test"
    else
        # Keep explicit include/skip label behavior for targeted local reruns.
        run "swift build" /usr/bin/swift build --package-path "$ROOT/macos" --scratch-path "$SWIFT_BUILD_DIR" -j "$AETOWER_BUILD_JOBS"
        run_swift_tests
    fi
}
