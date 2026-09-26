# Recipes carried by the camiloavelar/herdr fork. Imported from the root justfile.

# Open a PR merging herdrdev/herdr master into the fork (--dry-run, --continue, --skip-ci, --no-pr)
sync-upstream *args:
    bash scripts/fork/sync-upstream.sh {{args}}
