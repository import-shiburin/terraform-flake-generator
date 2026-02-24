mod constraint;
mod flake_check;
mod flake_generate;
mod flake_update;
mod hcl;
mod nixpkgs;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "tfg")]
#[command(about = "Generate Nix flakes from Terraform version constraints")]
struct Args {
    /// Terraform version to pin (e.g., 1.5.0)
    #[arg(value_name = "VERSION")]
    version: Option<String>,

    /// Terraform version to pin (alternative to positional argument)
    #[arg(long = "version", value_name = "VER", conflicts_with = "version")]
    version_flag: Option<String>,

    /// Working directory containing .tf files
    #[arg(long, default_value = ".")]
    dir: PathBuf,

    /// GitHub token for API access (or set GITHUB_TOKEN env var)
    #[arg(long, env = "GITHUB_TOKEN")]
    github_token: Option<String>,

    /// Show detailed search progress
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let dir = args.dir.canonicalize().context("invalid directory")?;
    let requested_version = args.version.or(args.version_flag);
    let verbose = args.verbose;

    // Step 1: Extract required_version from .tf files
    let constraint_str = hcl::extract_required_version(&dir)?;
    println!("Constraint: {}", constraint_str);

    let tf_constraint = constraint::VersionConstraint::parse(&constraint_str)?;

    // Determine the effective constraint to search with
    let search_constraint = if let Some(ref ver_str) = requested_version {
        let requested = constraint::Version::parse(ver_str)
            .with_context(|| format!("invalid version: {}", ver_str))?;

        if !tf_constraint.matches(&requested) {
            eprintln!(
                "Warning: {} does not satisfy constraint \"{}\"",
                requested, constraint_str
            );
        }

        constraint::VersionConstraint::parse(&format!("= {}", requested))?
    } else {
        tf_constraint
    };

    // Step 2: Check existing flake.nix
    let flake_path = dir.join("flake.nix");
    if flake_path.exists() {
        match flake_check::check(&dir, &search_constraint, args.github_token.as_deref())? {
            flake_check::CheckResult::Satisfied(version) => {
                println!("Existing flake.nix already satisfies constraint (Terraform {})", version);
                return Ok(());
            }
            flake_check::CheckResult::WrongVersion(version) => {
                println!("Existing flake.nix has Terraform {} (not a match)", version);
            }
            flake_check::CheckResult::NotFound => {
                println!("Existing flake.nix does not include Terraform");
            }
            flake_check::CheckResult::Unknown => {
                println!("Could not determine Terraform version in existing flake.nix");
            }
        }
    }

    // Step 3: Find nixpkgs commit with matching Terraform version
    if let Some(ref ver_str) = requested_version {
        println!("Searching nixpkgs for Terraform {}...", ver_str);
    } else {
        println!(
            "Searching nixpkgs for Terraform satisfying \"{}\"...",
            constraint_str
        );
    }
    let (version, commit) =
        nixpkgs::find_terraform_commit(&search_constraint, args.github_token.as_deref(), verbose)
            .with_context(|| {
                if let Some(ref ver_str) = requested_version {
                    format!("Terraform {} not found in nixpkgs", ver_str)
                } else {
                    format!(
                        "no Terraform version satisfying \"{}\" found in nixpkgs",
                        constraint_str
                    )
                }
            })?;
    println!(
        "Found Terraform {} at nixpkgs {}",
        version,
        &commit[..12]
    );

    // Step 4: Generate or update flake.nix
    if flake_path.exists() {
        flake_update::update(&dir, &commit)?;
        println!("Updated flake.nix");
    } else {
        flake_generate::generate(&dir, &commit)?;
        println!("Generated flake.nix");
    }

    Ok(())
}
