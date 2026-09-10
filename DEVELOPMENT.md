# Development Guide & Best Practices

This document outlines the Git flow and best practices for contributing to `devenv-linux`.

## Branching Strategy

We follow a modified Trunk-Based / Git Flow approach to keep development fast while ensuring releases are stable.

*   `main`: The source of truth for stable, released code. It should **only** accept merges from `dev`. Commits here always represent a working, tested product.
*   `dev`: The active integration branch. This is where all new features and bug fixes meet.
*   **Feature Branches (`feat/*`, `fix/*`, `chore/*`, `docs/*`)**: All development happens here. Branches are created from `dev`.

## Development Workflow

1.  **Branch:** Create a new branch off `dev` for your work.
    ```bash
    git checkout dev
    git pull
    git checkout -b feat/your-feature-name
    ```
2.  **Commit:** Make your changes locally. Commit messages must strictly adhere to the **Conventional Commits** specification (e.g., `feat: add collapsible groups to tui`, `fix: resolve sudo timeout bug`). This allows for automated changelog generation.
3.  **Pull Request (PR):** Push your branch and open a PR targeting the `dev` branch.
4.  **CI/CD Checks:** Opening a PR triggers the automated test matrix (`.github/workflows/test.yml`). Ensure all tests, formatting (`cargo fmt`), and linting (`cargo clippy`) pass.
5.  **Merge:** Once reviewed and CI passes, the PR is merged into `dev` using **Squash and Merge** to maintain a clean history.

## Release Process

When the `dev` branch is stable and a new version is ready to be published:

1.  **Prepare:** Fetch the remote branches and tags, merge `origin/main` into `dev`, and review any conflicts. Update the package version in both `installer/Cargo.toml` and `installer/Cargo.lock` to an unused release version, then commit and push `dev`.
2.  **Integration PR:** Create a PR from `dev` to `main`. Title it appropriately, e.g., `chore: release v1.2.0`. Wait for all CI checks to pass; PR checks run for both `dev` and `main` targets.
    *   **Important:** Use a standard **Merge Commit** for this PR (do not Squash or Rebase). This preserves the individual feature history from `dev` into `main` and allows Git to properly track the shared lineage between the two branches.
3.  **Tagging:** Once the PR is merged into `main`, pull `main` locally and create an annotated Git tag on the release merge commit. The version must match `installer/Cargo.toml`:
    ```bash
    git checkout main
    git pull --ff-only origin main
    git tag -a v1.2.0 -m "Release v1.2.0"
    git push origin v1.2.0
    ```
4.  **Automation:** The push of the `v*` tag automatically triggers `.github/workflows/release.yml`. This workflow will cross-compile the musl binaries and publish a new GitHub Release.
5.  **Synchronize:** Merge the released `main` back into `dev` so both branches share the release history and version:
    ```bash
    git checkout dev
    git merge origin/main
    git push origin dev
    ```
