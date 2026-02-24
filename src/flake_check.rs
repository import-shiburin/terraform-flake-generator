use crate::constraint::{Version, VersionConstraint};
use crate::nixpkgs;
use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug)]
pub enum CheckResult {
    /// Terraform is present and satisfies the constraint.
    Satisfied(Version),
    /// Terraform is present but does not satisfy the constraint.
    WrongVersion(Version),
    /// Terraform is not found in the flake.
    NotFound,
    /// Could not determine the terraform version.
    Unknown,
}

/// Check if an existing flake.nix provides a terraform version satisfying the constraint.
pub fn check(dir: &Path, constraint: &VersionConstraint, token: Option<&str>) -> Result<CheckResult> {
    let flake_nix_path = dir.join("flake.nix");
    let flake_source =
        std::fs::read_to_string(&flake_nix_path).context("failed to read flake.nix")?;

    // Check if terraform appears in the flake at all
    if !has_terraform(&flake_source) {
        return Ok(CheckResult::NotFound);
    }

    // Try to determine the pinned nixpkgs commit
    let commit = find_nixpkgs_commit(dir, &flake_source)?;
    let commit = match commit {
        Some(c) => c,
        None => return Ok(CheckResult::Unknown),
    };

    // Look up the terraform version at that commit
    let version_str = match nixpkgs::terraform_version_at_commit(&commit, token)? {
        Some(v) => v,
        None => return Ok(CheckResult::Unknown),
    };

    let version = Version::parse(&version_str)?;
    if constraint.matches(&version) {
        Ok(CheckResult::Satisfied(version))
    } else {
        Ok(CheckResult::WrongVersion(version))
    }
}

/// Check if the flake source contains terraform in buildInputs/packages.
fn has_terraform(source: &str) -> bool {
    // Walk the rnix CST to look for terraform identifiers in relevant contexts.
    // As a practical heuristic, check for `terraform` as a token in the source.
    let parse = rnix::Root::parse(source);
    let syntax = parse.syntax();

    for element in syntax.descendants_with_tokens() {
        if let rnix::NodeOrToken::Token(token) = element {
            if token.kind() == rnix::SyntaxKind::TOKEN_IDENT && token.text() == "terraform" {
                return true;
            }
        }
    }
    false
}

/// Try to find the pinned nixpkgs commit from flake.lock or flake.nix.
fn find_nixpkgs_commit(dir: &Path, flake_source: &str) -> Result<Option<String>> {
    // Try flake.lock first
    let lock_path = dir.join("flake.lock");
    if lock_path.exists() {
        let lock_content =
            std::fs::read_to_string(&lock_path).context("failed to read flake.lock")?;
        let lock: serde_json::Value =
            serde_json::from_str(&lock_content).context("failed to parse flake.lock")?;

        // Navigate: .nodes.nixpkgs.locked.rev
        if let Some(rev) = lock
            .get("nodes")
            .and_then(|n| n.get("nixpkgs"))
            .and_then(|n| n.get("locked"))
            .and_then(|n| n.get("rev"))
            .and_then(|v| v.as_str())
        {
            return Ok(Some(rev.to_string()));
        }
    }

    // Fallback: extract commit from nixpkgs URL in flake.nix
    // Look for github:NixOS/nixpkgs/<commit> pattern
    let re = regex::Regex::new(r"github:NixOS/nixpkgs/([a-f0-9]{40})").unwrap();
    if let Some(caps) = re.captures(flake_source) {
        return Ok(Some(caps[1].to_string()));
    }

    // Also try branch names like github:NixOS/nixpkgs/nixos-unstable
    let re = regex::Regex::new(r"github:NixOS/nixpkgs/([a-zA-Z0-9._-]+)").unwrap();
    if let Some(caps) = re.captures(flake_source) {
        return Ok(Some(caps[1].to_string()));
    }

    Ok(None)
}
