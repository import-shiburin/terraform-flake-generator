use anyhow::{bail, Context, Result};
use std::path::Path;

pub fn extract_required_version(dir: &Path) -> Result<String> {
    let pattern = dir.join("*.tf");
    let pattern_str = pattern.to_str().context("invalid directory path")?;

    let mut versions = Vec::new();

    for entry in glob::glob(pattern_str).context("invalid glob pattern")? {
        let path = entry.context("error reading glob entry")?;
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        // Skip files that don't mention required_version at all
        if !content.contains("required_version") {
            continue;
        }

        let body = hcl::parse(&content)
            .with_context(|| format!("failed to parse HCL in {}", path.display()))?;

        for block in body.blocks() {
            if block.identifier.to_string() == "terraform" {
                for attr in block.body.attributes() {
                    if attr.key.to_string() == "required_version" {
                        if let hcl::Expression::String(ref v) = attr.expr {
                            versions.push((v.clone(), path.display().to_string()));
                        }
                    }
                }
            }
        }
    }

    match versions.len() {
        0 => bail!("no required_version found in any .tf files in {}", dir.display()),
        1 => Ok(versions.into_iter().next().unwrap().0),
        _ => {
            let first = &versions[0].0;
            if versions.iter().all(|(v, _)| v == first) {
                Ok(versions.into_iter().next().unwrap().0)
            } else {
                let details: Vec<String> = versions
                    .iter()
                    .map(|(v, f)| format!("  {} in {}", v, f))
                    .collect();
                bail!(
                    "multiple conflicting required_version constraints found:\n{}",
                    details.join("\n")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_heredoc_in_list() {
        let input = r#"
resource "test" "x" {
  command = [
    "/bin/sh",
    "-c",
    <<-EOFTF
    echo hello
    EOFTF
  ]
}
"#;
        let body = hcl::parse(input);
        assert!(body.is_ok(), "heredoc in list failed: {:?}", body.err());
    }

    /// Verifies that the empty heredoc prefix bug is fixed in our local hcl-rs fork.
    /// An empty heredoc (closing marker on the line immediately after the opening)
    /// followed by a heredoc whose marker starts with the first marker's text
    /// (e.g. EOF → EOFTF) should parse correctly.
    #[test]
    fn test_hcl_rs_empty_heredoc_prefix_bug() {
        // empty EOF heredoc + EOFTF heredoc → should now succeed with local fix
        let buggy = "resource \"a\" \"a\" {\n  x = <<-EOF\n  EOF\n}\nresource \"b\" \"b\" {\n  y = <<-EOFTF\n  hello\n  EOFTF\n}\n";
        let result = hcl::parse(buggy);
        assert!(result.is_ok(), "empty heredoc prefix bug should be fixed: {:?}", result.err());

        // non-empty EOF heredoc + EOFTF heredoc → OK
        let ok = "resource \"a\" \"a\" {\n  x = <<-EOF\n  content\n  EOF\n}\nresource \"b\" \"b\" {\n  y = <<-EOFTF\n  hello\n  EOFTF\n}\n";
        assert!(hcl::parse(ok).is_ok());

        // empty EOF heredoc + unrelated MARKER heredoc → OK
        let ok2 = "resource \"a\" \"a\" {\n  x = <<-EOF\n  EOF\n}\nresource \"b\" \"b\" {\n  y = <<-MARKER\n  hello\n  MARKER\n}\n";
        assert!(hcl::parse(ok2).is_ok());
    }


}
