use anyhow::{bail, Context, Result};
use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        let parts: Vec<&str> = s.split('.').collect();
        match parts.len() {
            2 => Ok(Version {
                major: parts[0].parse().context("invalid major version")?,
                minor: parts[1].parse().context("invalid minor version")?,
                patch: 0,
            }),
            3 => Ok(Version {
                major: parts[0].parse().context("invalid major version")?,
                minor: parts[1].parse().context("invalid minor version")?,
                patch: parts[2].parse().context("invalid patch version")?,
            }),
            _ => bail!("invalid version format: {}", s),
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug)]
enum Comparator {
    Eq(Version),
    Neq(Version),
    Gt(Version),
    Gte(Version),
    Lt(Version),
    Lte(Version),
    /// ~> X.Y.Z means >= X.Y.Z and < X.(Y+1).0
    PessimisticPatch { major: u64, minor: u64, patch: u64 },
    /// ~> X.Y means >= X.Y.0 and < (X+1).0.0
    PessimisticMinor { major: u64, minor: u64 },
}

impl Comparator {
    fn matches(&self, v: &Version) -> bool {
        match self {
            Comparator::Eq(req) => v == req,
            Comparator::Neq(req) => v != req,
            Comparator::Gt(req) => v > req,
            Comparator::Gte(req) => v >= req,
            Comparator::Lt(req) => v < req,
            Comparator::Lte(req) => v <= req,
            Comparator::PessimisticPatch {
                major,
                minor,
                patch,
            } => {
                let lower = Version {
                    major: *major,
                    minor: *minor,
                    patch: *patch,
                };
                let upper = Version {
                    major: *major,
                    minor: minor + 1,
                    patch: 0,
                };
                v >= &lower && v < &upper
            }
            Comparator::PessimisticMinor { major, minor } => {
                let lower = Version {
                    major: *major,
                    minor: *minor,
                    patch: 0,
                };
                let upper = Version {
                    major: major + 1,
                    minor: 0,
                    patch: 0,
                };
                v >= &lower && v < &upper
            }
        }
    }
}

#[derive(Debug)]
pub struct VersionConstraint {
    comparators: Vec<Comparator>,
}

impl VersionConstraint {
    pub fn parse(s: &str) -> Result<Self> {
        let mut comparators = Vec::new();
        for part in s.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            comparators.push(parse_single(part)?);
        }
        if comparators.is_empty() {
            bail!("empty version constraint");
        }
        Ok(VersionConstraint { comparators })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.comparators.iter().all(|c| c.matches(version))
    }

    /// Pick the best (highest) version from candidates that satisfies this constraint.
    pub fn best_match<'a>(&self, candidates: &'a [(Version, String)]) -> Option<&'a (Version, String)> {
        candidates
            .iter()
            .filter(|(v, _)| self.matches(v))
            .max_by(|(a, _), (b, _)| a.cmp(b))
    }
}

fn parse_single(s: &str) -> Result<Comparator> {
    let s = s.trim();

    if let Some(rest) = s.strip_prefix("~>") {
        let rest = rest.trim();
        let parts: Vec<&str> = rest.split('.').collect();
        match parts.len() {
            2 => {
                let major: u64 = parts[0].parse().context("invalid major")?;
                let minor: u64 = parts[1].parse().context("invalid minor")?;
                Ok(Comparator::PessimisticMinor { major, minor })
            }
            3 => {
                let major: u64 = parts[0].parse().context("invalid major")?;
                let minor: u64 = parts[1].parse().context("invalid minor")?;
                let patch: u64 = parts[2].parse().context("invalid patch")?;
                Ok(Comparator::PessimisticPatch {
                    major,
                    minor,
                    patch,
                })
            }
            _ => bail!("invalid pessimistic constraint: {}", s),
        }
    } else if let Some(rest) = s.strip_prefix(">=") {
        Ok(Comparator::Gte(Version::parse(rest)?))
    } else if let Some(rest) = s.strip_prefix("<=") {
        Ok(Comparator::Lte(Version::parse(rest)?))
    } else if let Some(rest) = s.strip_prefix("!=") {
        Ok(Comparator::Neq(Version::parse(rest)?))
    } else if let Some(rest) = s.strip_prefix('>') {
        Ok(Comparator::Gt(Version::parse(rest)?))
    } else if let Some(rest) = s.strip_prefix('<') {
        Ok(Comparator::Lt(Version::parse(rest)?))
    } else if let Some(rest) = s.strip_prefix('=') {
        Ok(Comparator::Eq(Version::parse(rest)?))
    } else {
        // Bare version = exact match
        Ok(Comparator::Eq(Version::parse(s)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pessimistic_patch() {
        let c = VersionConstraint::parse("~> 1.5.0").unwrap();
        assert!(c.matches(&Version::parse("1.5.0").unwrap()));
        assert!(c.matches(&Version::parse("1.5.7").unwrap()));
        assert!(!c.matches(&Version::parse("1.6.0").unwrap()));
        assert!(!c.matches(&Version::parse("1.4.9").unwrap()));
    }

    #[test]
    fn test_pessimistic_minor() {
        let c = VersionConstraint::parse("~> 1.5").unwrap();
        assert!(c.matches(&Version::parse("1.5.0").unwrap()));
        assert!(c.matches(&Version::parse("1.9.9").unwrap()));
        assert!(!c.matches(&Version::parse("2.0.0").unwrap()));
        assert!(!c.matches(&Version::parse("1.4.9").unwrap()));
    }

    #[test]
    fn test_compound() {
        let c = VersionConstraint::parse(">= 1.3.0, < 2.0.0").unwrap();
        assert!(c.matches(&Version::parse("1.5.0").unwrap()));
        assert!(c.matches(&Version::parse("1.3.0").unwrap()));
        assert!(!c.matches(&Version::parse("1.2.9").unwrap()));
        assert!(!c.matches(&Version::parse("2.0.0").unwrap()));
    }

    #[test]
    fn test_exact() {
        let c = VersionConstraint::parse("= 1.5.0").unwrap();
        assert!(c.matches(&Version::parse("1.5.0").unwrap()));
        assert!(!c.matches(&Version::parse("1.5.1").unwrap()));
    }
}
