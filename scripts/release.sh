#!/bin/bash
set -euo pipefail

# Usage: ./scripts/release.sh [major|minor|patch]
# Bumps version in Cargo.toml, commits, tags, and pushes.

BUMP_TYPE="${1:-patch}"

# Get current version from workspace Cargo.toml
CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

case "$BUMP_TYPE" in
    major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
    minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
    patch) PATCH=$((PATCH + 1)) ;;
    *) echo "Usage: $0 [major|minor|patch]"; exit 1 ;;
esac

NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}"
TAG="v${NEW_VERSION}"

echo "Bumping: ${CURRENT} -> ${NEW_VERSION}"

# Update version in workspace Cargo.toml
sed -i "s/^version = \"${CURRENT}\"/version = \"${NEW_VERSION}\"/" Cargo.toml

# Commit and tag
git add Cargo.toml
git commit -m "Release ${TAG}"
git tag -a "$TAG" -m "Release ${TAG}"

echo ""
echo "Created tag: ${TAG}"
echo ""
echo "Push with:"
echo "  git push && git push --tags"
