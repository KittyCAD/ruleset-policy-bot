#!/usr/bin/env bash
set -euo pipefail

# Configuration
BRANCH="ci-tests"
COMMIT_HASH="b67a1e80cda53b287d0e01f00a6932d0704c42c2"
REMOTE="origin"


# Create a temp working directory under /tmp
WORKDIR=$(mktemp -d /tmp/ci-tests-reset.XXXXXX)
echo "Working in $WORKDIR"

# Determine repo URL based on environment (GitHub Actions uses HTTPS)
if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
  REPO_URL="https://github.com/KittyCAD/ruleset-policy-bot.git"
else
  REPO_URL="git@github.com:KittyCAD/ruleset-policy-bot.git"
fi

# Clone the repository into the temp directory
git clone "$REPO_URL" "$WORKDIR"
cd "$WORKDIR"
git fetch

# Ensure branch exists remotely (optional; we will (re)create it anyway)
# Check out/reset branch to the specific commit (create/update branch directly)
git checkout --detach "$COMMIT_HASH"
# Create or reset the branch name to point at this commit
git branch -f "$BRANCH" "$COMMIT_HASH"
# Switch to the branch
git checkout "$BRANCH"

# Create an empty commit
git commit --allow-empty -m "ci: empty commit violate ruleset"

# Force push to remote
git push -f "$REMOTE" "$BRANCH"

echo "Done: ${BRANCH} reset to ${COMMIT_HASH}, empty commit added, and force pushed to ${REMOTE}. Repo cloned to ${WORKDIR}."
