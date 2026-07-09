#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${AETOWER_RELEASE_ENV_FILE:-$ROOT/.env.release.local}"

usage() {
    cat <<EOF
usage: $0 [--tap owner/name] [--tap-dir path] [--repo-url url] [--create] [--no-push] [--skip-brew-checks]

Publish the generated Homebrew cask into a real tap repository.

Defaults:
  tap:       Aeptus/aetower
  tap repo:  https://github.com/Aeptus/homebrew-aetower.git
  cask:      dist/homebrew/Casks/aetower.rb
EOF
}

run_brew_checks() {
    if command -v brew >/dev/null 2>&1 && [ "$RUN_BREW_CHECKS" -eq 1 ]; then
        brew tap "$TAP_INSTALL" "$TAP_REPO_URL"
        brew style --cask "$TAP_INSTALL/$CASK_TOKEN"
        brew audit --cask --online "$TAP_INSTALL/$CASK_TOKEN"
    elif [ "$RUN_BREW_CHECKS" -eq 1 ]; then
        echo "brew not found; skipping Homebrew style/audit checks" >&2
    fi
}

if [ -f "$ENV_FILE" ]; then
    set -a
    # shellcheck disable=SC1090
    . "$ENV_FILE"
    set +a
fi

CASK_TOKEN="${AETOWER_HOMEBREW_CASK_TOKEN:-aetower}"
CASK_PATH="${AETOWER_HOMEBREW_CASK_PATH:-$ROOT/dist/homebrew/Casks/$CASK_TOKEN.rb}"
TAP="${AETOWER_HOMEBREW_TAP:-Aeptus/aetower}"
TAP_DIR="${AETOWER_HOMEBREW_TAP_DIR:-$ROOT/dist/homebrew-tap}"
TAP_REPO_URL="${AETOWER_HOMEBREW_TAP_REPO_URL:-}"
TAP_BRANCH="${AETOWER_HOMEBREW_TAP_BRANCH:-main}"
CREATE_REPO=0
PUSH=1
RUN_BREW_CHECKS="${AETOWER_HOMEBREW_RUN_BREW_CHECKS:-1}"

while [ "$#" -gt 0 ]; do
    case "$1" in
        --tap)
            TAP="$2"
            shift 2
            ;;
        --tap-dir)
            TAP_DIR="$2"
            shift 2
            ;;
        --repo-url)
            TAP_REPO_URL="$2"
            shift 2
            ;;
        --create)
            CREATE_REPO=1
            shift
            ;;
        --no-push)
            PUSH=0
            shift
            ;;
        --skip-brew-checks)
            RUN_BREW_CHECKS=0
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unsupported argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

case "$TAP" in
    */*) ;;
    *)
        echo "tap must be in owner/name form: $TAP" >&2
        exit 1
        ;;
esac

TAP_OWNER="${TAP%%/*}"
TAP_NAME="${TAP#*/}"
case "$TAP_NAME" in
    homebrew-*)
        TAP_REPO_NAME="$TAP_NAME"
        TAP_SHORT_NAME="${TAP_NAME#homebrew-}"
        ;;
    *)
        TAP_REPO_NAME="homebrew-$TAP_NAME"
        TAP_SHORT_NAME="$TAP_NAME"
        ;;
esac
TAP_SHORT="$TAP_OWNER/$TAP_SHORT_NAME"
TAP_INSTALL="$(printf '%s' "$TAP_SHORT" | tr '[:upper:]' '[:lower:]')"

if [ -z "$TAP_REPO_URL" ]; then
    TAP_REPO_URL="https://github.com/$TAP_OWNER/$TAP_REPO_NAME.git"
fi

if [ ! -f "$CASK_PATH" ]; then
    echo "missing Homebrew cask: $CASK_PATH" >&2
    echo "run sh scripts/generate-homebrew-cask.sh first" >&2
    exit 1
fi

ruby -c "$CASK_PATH" >/dev/null

if [ ! -d "$TAP_DIR/.git" ]; then
    mkdir -p "$(dirname "$TAP_DIR")"
    if ! git ls-remote "$TAP_REPO_URL" >/dev/null 2>&1; then
        if [ "$CREATE_REPO" -ne 1 ]; then
            echo "tap repository does not exist or is not reachable: $TAP_REPO_URL" >&2
            echo "create it first, or rerun with --create when gh is authenticated" >&2
            exit 1
        fi
        gh repo create "$TAP_OWNER/$TAP_REPO_NAME" \
            --public \
            --description "Homebrew tap for Aetower"
    fi
    git clone "$TAP_REPO_URL" "$TAP_DIR"
fi

REMOTE_URL="$(git -C "$TAP_DIR" remote get-url origin)"
if [ "$REMOTE_URL" != "$TAP_REPO_URL" ]; then
    echo "tap checkout origin mismatch:" >&2
    echo "  expected: $TAP_REPO_URL" >&2
    echo "  actual:   $REMOTE_URL" >&2
    exit 1
fi

if git -C "$TAP_DIR" rev-parse --verify HEAD >/dev/null 2>&1; then
    git -C "$TAP_DIR" fetch origin
    DEFAULT_BRANCH="$(git -C "$TAP_DIR" symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's#^refs/remotes/origin/##' || true)"
    if [ -z "$DEFAULT_BRANCH" ]; then
        DEFAULT_BRANCH="$TAP_BRANCH"
    fi
    git -C "$TAP_DIR" checkout "$DEFAULT_BRANCH"
    git -C "$TAP_DIR" pull --ff-only origin "$DEFAULT_BRANCH"
else
    git -C "$TAP_DIR" checkout -B "$TAP_BRANCH"
    DEFAULT_BRANCH="$TAP_BRANCH"
fi

mkdir -p "$TAP_DIR/Casks"
cp "$CASK_PATH" "$TAP_DIR/Casks/$CASK_TOKEN.rb"

cat > "$TAP_DIR/README.md" <<EOF
# Aetower Homebrew Tap

Install Aetower from the official Homebrew tap:

\`\`\`sh
brew tap $TAP_INSTALL
brew install --cask $CASK_TOKEN
\`\`\`

The cask installs the same signed and notarized Aetower app bundle published at
https://aetower.dev/ and links the bundled \`aetower\` CLI into Homebrew's
\`bin\` directory.

After installation, launch Aetower and smoke-check:

\`\`\`sh
aetower top
aetower storage
aetower repos
\`\`\`
EOF

git -C "$TAP_DIR" add README.md "Casks/$CASK_TOKEN.rb"

if git -C "$TAP_DIR" diff --cached --quiet; then
    printf 'Homebrew tap already up to date: %s\n' "$TAP_INSTALL"
    run_brew_checks
    exit 0
fi

VERSION="$(sed -n 's/^[[:space:]]*version "\(.*\)".*$/\1/p' "$CASK_PATH" | head -n 1)"
git -C "$TAP_DIR" commit -m "Update Aetower cask to $VERSION"

if [ "$PUSH" -eq 1 ]; then
    git -C "$TAP_DIR" push -u origin "$DEFAULT_BRANCH"
    printf 'Homebrew tap published: https://github.com/%s/%s\n' "$TAP_OWNER" "$TAP_REPO_NAME"
    run_brew_checks
else
    printf 'Homebrew tap commit prepared without push: %s\n' "$TAP_DIR"
    if [ "$RUN_BREW_CHECKS" -eq 1 ]; then
        printf 'Homebrew style/audit checks run after the tap is pushed and tapped locally.\n'
    fi
fi

printf 'Install with:\n'
printf '  brew tap %s\n' "$TAP_INSTALL"
printf '  brew install --cask %s\n' "$CASK_TOKEN"
