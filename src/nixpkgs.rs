use crate::constraint::{Version, VersionConstraint};
use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::Deserialize;

const TERRAFORM_PATHS: &[&str] = &[
    "pkgs/by-name/te/terraform/package.nix",
    "pkgs/applications/networking/cluster/terraform/default.nix",
];

#[derive(Deserialize)]
struct CommitInfo {
    sha: String,
}

#[derive(Deserialize)]
struct CommitListEntry {
    sha: String,
}

#[derive(Deserialize)]
struct GitRef {
    #[serde(rename = "ref")]
    ref_name: String,
    object: GitRefObject,
}

#[derive(Deserialize)]
struct GitRefObject {
    sha: String,
}

fn make_client(token: Option<&str>) -> Result<reqwest::blocking::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        "application/vnd.github.v3+json".parse().unwrap(),
    );
    headers.insert(
        reqwest::header::USER_AGENT,
        "terraform-flake-generator".parse().unwrap(),
    );
    if let Some(token) = token {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", token).parse().context("invalid token")?,
        );
    }
    reqwest::blocking::Client::builder()
        .default_headers(headers)
        .build()
        .context("failed to create HTTP client")
}

/// Extract terraform version from a Nix expression source using regex.
fn extract_version_from_nix(source: &str) -> Option<String> {
    let re = Regex::new(r#"version\s*=\s*"(\d+\.\d+\.\d+)""#).unwrap();
    re.captures(source).map(|c| c[1].to_string())
}

/// Fetch the terraform Nix expression at a specific nixpkgs ref.
/// Tries multiple known paths.
fn fetch_terraform_nix(
    client: &reqwest::blocking::Client,
    nixpkgs_ref: &str,
) -> Result<Option<String>> {
    for path in TERRAFORM_PATHS {
        let url = format!(
            "https://raw.githubusercontent.com/NixOS/nixpkgs/{}/{}",
            nixpkgs_ref, path
        );
        let resp = client.get(&url).send().context("HTTP request failed")?;
        if resp.status().is_success() {
            let body = resp.text().context("failed to read response body")?;
            return Ok(Some(body));
        }
    }
    Ok(None)
}

/// Resolve a branch name to its HEAD commit SHA.
fn resolve_branch_sha(client: &reqwest::blocking::Client, branch: &str) -> Result<String> {
    let url = format!(
        "https://api.github.com/repos/NixOS/nixpkgs/commits/{}",
        branch
    );
    let resp = client.get(&url).send().context("GitHub API request failed")?;
    if !resp.status().is_success() {
        bail!(
            "failed to resolve branch {}: HTTP {}",
            branch,
            resp.status()
        );
    }
    let info: CommitInfo = resp.json().context("failed to parse commit info")?;
    Ok(info.sha)
}

/// Fetch recent nixpkgs branches dynamically from GitHub.
/// Returns `(branch_name, sha)` pairs: `nixpkgs-unstable` followed by the 5 most
/// recent `nixos-YY.MM` release branches.
fn fetch_recent_branches(
    client: &reqwest::blocking::Client,
    verbose: bool,
) -> Result<Vec<(String, String)>> {
    let url = "https://api.github.com/repos/NixOS/nixpkgs/git/matching-refs/heads/nixos-";
    let resp = client.get(url).send().context("GitHub matching-refs request failed")?;
    if !resp.status().is_success() {
        bail!("matching-refs API returned HTTP {}", resp.status());
    }
    let refs: Vec<GitRef> = resp.json().context("failed to parse matching-refs response")?;

    let re = Regex::new(r"^refs/heads/(nixos-\d{2}\.\d{2})$").unwrap();
    let mut release_branches: Vec<(String, String)> = refs
        .into_iter()
        .filter_map(|r| {
            re.captures(&r.ref_name)
                .map(|caps| (caps[1].to_string(), r.object.sha))
        })
        .collect();

    // Descending sort by name — fixed-width YY.MM format sorts correctly
    release_branches.sort_by(|a, b| b.0.cmp(&a.0));
    release_branches.truncate(5);

    // Prepend nixpkgs-unstable (need to resolve its SHA separately)
    let unstable_sha = resolve_branch_sha(client, "nixpkgs-unstable")?;
    let mut branches = vec![("nixpkgs-unstable".to_string(), unstable_sha)];
    branches.extend(release_branches);

    if verbose {
        eprintln!(
            "Discovered {} branches: {}",
            branches.len(),
            branches.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
        );
    }

    Ok(branches)
}

/// Fetch the terraform version at a specific nixpkgs commit.
pub fn terraform_version_at_commit(
    commit: &str,
    token: Option<&str>,
) -> Result<Option<String>> {
    let client = make_client(token)?;
    let nix_source = fetch_terraform_nix(&client, commit)?;
    Ok(nix_source.and_then(|s| extract_version_from_nix(&s)))
}

/// Find a nixpkgs commit that provides a terraform version satisfying the constraint.
/// Returns (version, commit_sha).
pub fn find_terraform_commit(
    constraint: &VersionConstraint,
    token: Option<&str>,
    verbose: bool,
) -> Result<(Version, String)> {
    let client = make_client(token)?;
    let mut candidates: Vec<(Version, String)> = Vec::new();

    // Tier 1: Check branch HEADs
    let branches = fetch_recent_branches(&client, verbose)?;
    if verbose {
        eprintln!("Checking nixpkgs branch HEADs...");
    }
    for (branch, sha) in &branches {
        if verbose {
            eprint!("  {}... ", branch);
        }

        let nix_source = match fetch_terraform_nix(&client, sha)? {
            Some(s) => s,
            None => {
                if verbose {
                    eprintln!("terraform package not found");
                }
                continue;
            }
        };

        let version_str = match extract_version_from_nix(&nix_source) {
            Some(v) => v,
            None => {
                if verbose {
                    eprintln!("could not extract version");
                }
                continue;
            }
        };

        let version = match Version::parse(&version_str) {
            Ok(v) => v,
            Err(_) => {
                if verbose {
                    eprintln!("invalid version: {}", version_str);
                }
                continue;
            }
        };

        if verbose {
            eprintln!("terraform {} ({})", version, &sha[..12]);
        }

        if constraint.matches(&version) {
            candidates.push((version, sha.clone()));
        }
    }

    // If we found matches in tier 1, pick the best
    if let Some((version, sha)) = constraint.best_match(&candidates) {
        return Ok((version.clone(), sha.clone()));
    }

    // Tier 2: Walk commit history
    if verbose {
        eprintln!("No match in branch HEADs, walking commit history...");
    }
    for path in TERRAFORM_PATHS {
        let url = format!(
            "https://api.github.com/repos/NixOS/nixpkgs/commits?path={}&per_page=100",
            path
        );
        let resp = client.get(&url).send().context("GitHub API request failed")?;
        if !resp.status().is_success() {
            continue;
        }

        let commits: Vec<CommitListEntry> =
            resp.json().context("failed to parse commits list")?;

        for commit in &commits {
            let nix_source = match fetch_terraform_nix(&client, &commit.sha)? {
                Some(s) => s,
                None => continue,
            };

            let version_str = match extract_version_from_nix(&nix_source) {
                Some(v) => v,
                None => continue,
            };

            let version = match Version::parse(&version_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if verbose {
                eprint!("  {} terraform {}... ", &commit.sha[..12], version);
            }

            if constraint.matches(&version) {
                if verbose {
                    eprintln!("match!");
                }
                return Ok((version, commit.sha.clone()));
            } else if verbose {
                eprintln!("no match");
            }
        }
    }

    bail!("could not find a nixpkgs commit with a terraform version satisfying the constraint")
}
