#!/usr/bin/env bash
# Open a pull request that merges herdrdev/herdr master into the fork's master.
#
#   just sync-upstream              merge, run local CI, push branch, open PR
#   just sync-upstream --dry-run    report divergence and predicted conflicts only
#   just sync-upstream --continue   after resolving conflicts by hand in the worktree
#   just sync-upstream --skip-ci    skip local CI (GitHub CI still runs on the PR)
#   just sync-upstream --no-pr      stop after the local merge (no push, no PR)
#
# Upstream tags are never fetched. The merge happens in a separate worktree, so
# the main checkout and its branch are untouched. git rerere replays conflict
# resolutions recorded by earlier syncs.
#
# Merge the PR with "Create a merge commit". Squash or rebase drops the link to
# upstream history and the next sync conflicts on everything again.
set -euo pipefail

UPSTREAM_URL="${HERDR_UPSTREAM_URL:-https://github.com/herdrdev/herdr.git}"
UPSTREAM_REF="refs/remotes/upstream/master"
BASE_REF="origin/master"

dry_run=false
resume=false
skip_ci=false
open_pr=true
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry_run=true ;;
        --continue) resume=true ;;
        --skip-ci) skip_ci=true ;;
        --no-pr) open_pr=false ;;
        -h | --help)
            sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "error: unknown option $arg" >&2
            exit 2
            ;;
    esac
done

repo_root="$(git rev-parse --show-toplevel)"
worktree="${HERDR_SYNC_WORKTREE:-$(dirname "$repo_root")/herdr-worktrees/upstream-sync}"
cd "$repo_root"

say() { printf '==> %s\n' "$*"; }
die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

# Keep recorded resolutions and let rerere stage the files it resolves.
git config rerere.enabled true
git config rerere.autoupdate true

origin_repo() {
    git remote get-url origin | sed -E 's#^(git@github\.com:|https://github\.com/)##; s#\.git$##'
}

run_ci() {
    if [ "$skip_ci" = true ]; then
        say "skipping local CI (--skip-ci)"
        return
    fi
    say "running local CI in $worktree"
    local runner=()
    command -v mise >/dev/null 2>&1 && runner=(mise x --)
    (
        cd "$worktree"
        # Build against the main checkout's target dir so the sync does not rebuild from scratch.
        export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_root/target}"
        if [ "$(uname -s)" = Darwin ] && [ -z "${SDKROOT:-}" ] && command -v xcrun >/dev/null 2>&1; then
            SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
            export SDKROOT
        fi
        "${runner[@]}" just ci
        "${runner[@]}" just docs-contract-test
    )
}

publish() {
    local branch subject
    branch="$(git -C "$worktree" rev-parse --abbrev-ref HEAD)"
    subject="$(git -C "$worktree" log -1 --format=%s)"
    printf '%s\n' "$subject" | python3 scripts/conventional_commits.py >/dev/null ||
        die "merge commit subject is not a conventional commit: $subject"
    if [ "$open_pr" = false ]; then
        say "merge ready on $branch in $worktree (--no-pr: not pushed)"
        return
    fi
    local repo
    repo="$(origin_repo)"
    say "pushing $branch to $repo"
    git -C "$worktree" push -u origin "$branch"
    local body
    body="$(
        cat <<EOF
Merges herdrdev/herdr master into the fork (no upstream tags).

$(git -C "$worktree" log --format='- %s' --no-merges "HEAD^1..HEAD^2" | head -n 60)

Merge with **Create a merge commit**. Squash or rebase breaks the next upstream sync.
EOF
    )"
    gh pr create -R "$repo" --base master --head "$branch" --title "$subject" --body "$body"
    # The branch lives on in the PR; fix CI failures by checking it out.
    git worktree remove "$worktree"
    say "done; worktree removed, branch $branch kept"
}

if [ "$resume" = true ]; then
    [ -d "$worktree" ] || die "no sync worktree at $worktree; start a new sync without --continue"
    marked=()
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        if grep -qE '^(<<<<<<<|>>>>>>>) ' "$worktree/$path" 2>/dev/null; then
            marked+=("$path")
        else
            git -C "$worktree" add -- "$path"
        fi
    done < <(git -C "$worktree" diff --name-only --diff-filter=U)
    if [ "${#marked[@]}" -gt 0 ]; then
        die "conflict markers remain in $worktree:
$(printf '    %s\n' "${marked[@]}")"
    fi
    if git -C "$worktree" rev-parse -q --verify MERGE_HEAD >/dev/null; then
        # Committing records the resolution, so rerere replays it on the next sync.
        git -C "$worktree" commit --no-edit
    fi
    run_ci
    publish
    exit 0
fi

say "fetching origin and upstream master (no tags)"
git fetch --no-tags origin
git fetch --no-tags "$UPSTREAM_URL" "master:$UPSTREAM_REF"

behind="$(git rev-list --count "$BASE_REF..$UPSTREAM_REF")"
ahead="$(git rev-list --count "$UPSTREAM_REF..$BASE_REF")"
say "upstream has $behind new commit(s); fork is $ahead commit(s) ahead"
if [ "$behind" -eq 0 ]; then
    say "already up to date"
    exit 0
fi

if [ "$dry_run" = true ]; then
    if conflicts="$(git merge-tree --write-tree --name-only "$BASE_REF" "$UPSTREAM_REF" 2>/dev/null)"; then
        say "merge would be clean"
    else
        say "files with textual conflicts (before rerere):"
        printf '%s\n' "$conflicts" | sed -n '2,/^$/p' | sed '/^$/d; s/^/    /'
    fi
    exit 0
fi

if [ -e "$worktree" ]; then
    die "$worktree already exists; finish it with --continue or remove it with: git worktree remove --force $worktree"
fi

upstream_sha="$(git rev-parse --short=12 "$UPSTREAM_REF")"
upstream_version="$(git show "$UPSTREAM_REF:Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p' | head -n 1)"
branch="upstream-sync/$(date +%Y-%m-%d)-$upstream_sha"
git show-ref --quiet "refs/heads/$branch" && die "branch $branch already exists"

say "creating worktree $worktree on $branch"
mkdir -p "$(dirname "$worktree")"
git worktree add -q -b "$branch" "$worktree" "$BASE_REF"

message="chore: merge upstream master ${upstream_version:+(herdr $upstream_version) }at $upstream_sha"
if ! git -C "$worktree" merge --no-ff --no-edit -m "$message" "$UPSTREAM_REF"; then
    unresolved="$(git -C "$worktree" diff --name-only --diff-filter=U)"
    if [ -n "$unresolved" ]; then
        cat >&2 <<EOF

Conflicts need a manual resolution in $worktree:
$(printf '%s\n' "$unresolved" | sed 's/^/    /')

Resolve them there, then run: just sync-upstream --continue
EOF
        exit 1
    fi
    # rerere resolved and staged every conflict; conclude the merge.
    git -C "$worktree" commit --no-edit
    say "conflicts resolved from recorded rerere resolutions"
fi

run_ci
publish
