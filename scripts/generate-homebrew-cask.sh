#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
APP_DIR="${AETOWER_APP_DIR:-$DIST_DIR/Aetower.app}"
APPCAST_DIR="${AETOWER_APPCAST_DIR:-$DIST_DIR/appcast}"
HOMEBREW_DIR="${AETOWER_HOMEBREW_DIR:-$DIST_DIR/homebrew/Casks}"
CASK_PATH="${AETOWER_HOMEBREW_CASK_PATH:-$HOMEBREW_DIR/aetower.rb}"
DOWNLOAD_PREFIX="${AETOWER_HOMEBREW_DOWNLOAD_URL_PREFIX:-${AETOWER_DOWNLOAD_URL_PREFIX:-https://aetower.dev/releases/}}"
APPCAST_URL="${AETOWER_HOMEBREW_APPCAST_URL:-${AETOWER_APPCAST_URL:-https://aetower.dev/releases/appcast.xml}}"

if [ ! -d "$APP_DIR" ]; then
    echo "missing app bundle: $APP_DIR" >&2
    echo "run sh scripts/release-candidate.sh first" >&2
    exit 1
fi

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_DIR/Contents/Info.plist")"
BUILD_NUMBER="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$APP_DIR/Contents/Info.plist")"

case "$DOWNLOAD_PREFIX" in
    */) ;;
    *) DOWNLOAD_PREFIX="$DOWNLOAD_PREFIX/" ;;
esac

ARCHIVE_NAME="Aetower-$VERSION-$BUILD_NUMBER.zip"
ARCHIVE_PATH="$APPCAST_DIR/$ARCHIVE_NAME"
if [ ! -f "$ARCHIVE_PATH" ]; then
    echo "missing immutable Homebrew archive: $ARCHIVE_PATH" >&2
    echo "run sh scripts/generate-sparkle-appcast.sh after packaging" >&2
    exit 1
fi

SHA256="$(shasum -a 256 "$ARCHIVE_PATH" | awk '{ print $1 }')"

mkdir -p "$HOMEBREW_DIR"
cat > "$CASK_PATH" <<CASK
cask "aetower" do
  version "$VERSION,$BUILD_NUMBER"
  sha256 "$SHA256"

  url "${DOWNLOAD_PREFIX}Aetower-#{version.csv.first}-#{version.csv.second}.zip"
  name "Aetower"
  desc "Local-first observability for operators, developers, and AI agents"
  homepage "https://aetower.dev/"

  livecheck do
    url "$APPCAST_URL"
    strategy :sparkle
  end

  auto_updates true
  depends_on macos: :sonoma

  app "Aetower.app"
  binary "#{appdir}/Aetower.app/Contents/Helpers/aetower",
         target: "aetower"

  zap trash: [
    "~/Library/Application Support/Aetower",
    "~/Library/Caches/com.aeptus.aetower",
    "~/Library/Logs/Aetower",
    "~/Library/Preferences/com.aeptus.aetower.plist",
  ]

  caveats <<~EOS
    Homebrew links the bundled \`aetower\` command to:
      \$(brew --prefix)/bin/aetower
    Keep Aetower.app running, then try:
      aetower top
      aetower storage
      aetower repos
  EOS
end
CASK

printf 'homebrew cask written: %s\n' "$CASK_PATH"
printf 'version: %s,%s\n' "$VERSION" "$BUILD_NUMBER"
printf 'sha256: %s\n' "$SHA256"
