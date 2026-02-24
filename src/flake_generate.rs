use anyhow::{Context, Result};
use std::path::Path;

pub fn generate(dir: &Path, commit_sha: &str) -> Result<()> {
    let content = format!(
        r#"{{
  description = "Development environment";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/{}";
    flake-parts.url = "github:hercules-ci/flake-parts";
  }};

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake {{ inherit inputs; }} {{
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      perSystem = {{ pkgs, ... }}: {{
        devShells.default = pkgs.mkShell {{
          buildInputs = [
            pkgs.terraform
          ];
        }};
      }};
    }};
}}
"#,
        commit_sha
    );

    let path = dir.join("flake.nix");
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
