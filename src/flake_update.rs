use anyhow::{bail, Context, Result};
use rnix::SyntaxKind;
use std::path::Path;

/// Update an existing flake.nix: replace the nixpkgs commit and optionally add terraform.
pub fn update(dir: &Path, new_commit: &str) -> Result<()> {
    let flake_path = dir.join("flake.nix");
    let source =
        std::fs::read_to_string(&flake_path).context("failed to read flake.nix")?;

    let mut result = source.clone();

    // Step 1: Replace the nixpkgs URL commit
    result = replace_nixpkgs_url(&result, new_commit)?;

    // Step 2: Add terraform to buildInputs if not present
    if !has_terraform_in_build_inputs(&result) {
        result = add_terraform_to_build_inputs(&result)?;
    }

    std::fs::write(&flake_path, result)
        .with_context(|| format!("failed to write {}", flake_path.display()))?;
    Ok(())
}

/// Replace the nixpkgs URL in the flake source using rnix CST for precise location.
fn replace_nixpkgs_url(source: &str, new_commit: &str) -> Result<String> {
    let parse = rnix::Root::parse(source);
    let syntax = parse.syntax();

    // Find the STRING_CONTENT token containing the nixpkgs URL
    for element in syntax.descendants_with_tokens() {
        if let rnix::NodeOrToken::Token(token) = element {
            if token.kind() == SyntaxKind::TOKEN_STRING_CONTENT {
                let text = token.text();
                if text.contains("github:NixOS/nixpkgs/") {
                    let range = token.text_range();
                    let start: usize = range.start().into();
                    let end: usize = range.end().into();

                    let new_url = format!("github:NixOS/nixpkgs/{}", new_commit);
                    let mut result = String::with_capacity(source.len());
                    result.push_str(&source[..start]);
                    result.push_str(&new_url);
                    result.push_str(&source[end..]);
                    return Ok(result);
                }
            }
        }
    }

    bail!("could not find nixpkgs URL in flake.nix")
}

/// Check if terraform already appears in a buildInputs list.
fn has_terraform_in_build_inputs(source: &str) -> bool {
    let parse = rnix::Root::parse(source);
    let syntax = parse.syntax();

    // Look for `terraform` identifier that's a child of a list inside buildInputs
    // As a practical approach: find any `terraform` ident token
    for element in syntax.descendants_with_tokens() {
        if let rnix::NodeOrToken::Token(token) = element {
            if token.kind() == SyntaxKind::TOKEN_IDENT && token.text() == "terraform" {
                return true;
            }
        }
    }
    false
}

/// Add `pkgs.terraform` to the buildInputs list in flake.nix.
fn add_terraform_to_build_inputs(source: &str) -> Result<String> {
    let parse = rnix::Root::parse(source);
    let syntax = parse.syntax();

    // Strategy: find the buildInputs attrpath, then find its list value,
    // then find the closing bracket and insert before it.

    // Walk to find `buildInputs` identifier
    let mut found_build_inputs = false;
    for node in syntax.descendants() {
        if node.kind() == SyntaxKind::NODE_ATTRPATH_VALUE {
            // Check if the attrpath contains "buildInputs"
            let text = node.text().to_string();
            if text.contains("buildInputs") {
                found_build_inputs = true;

                // Find the list node within this attrpath_value
                for child in node.descendants() {
                    if child.kind() == SyntaxKind::NODE_LIST {
                        // Find the closing bracket token
                        for token in child.children_with_tokens() {
                            if let rnix::NodeOrToken::Token(t) = &token {
                                if t.kind() == SyntaxKind::TOKEN_R_BRACK {
                                    let pos: usize = t.text_range().start().into();
                                    // Determine indentation from context
                                    let indent = detect_list_indent(source, pos);
                                    let insertion =
                                        format!("{}pkgs.terraform\n{}", indent, &indent[..indent.len().saturating_sub(2)]);
                                    let mut result = String::with_capacity(source.len() + insertion.len());
                                    result.push_str(&source[..pos]);
                                    result.push_str(&insertion);
                                    result.push_str(&source[pos..]);
                                    return Ok(result);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !found_build_inputs {
        bail!("could not find buildInputs in flake.nix");
    }

    bail!("could not find list in buildInputs")
}

/// Detect the indentation used for list items by looking at the context before the bracket.
fn detect_list_indent(source: &str, bracket_pos: usize) -> String {
    // Look backwards from the bracket position to find the line start
    let before = &source[..bracket_pos];
    if let Some(last_newline) = before.rfind('\n') {
        let line = &before[last_newline + 1..bracket_pos];
        // The bracket line's indentation, plus two more spaces for the item
        let base_indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        format!("{}  ", base_indent)
    } else {
        "            ".to_string()
    }
}
