use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use glob::Pattern;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleAction {
    ReadOnly,
    ReadWrite,
    Deny,
}

#[derive(Debug, Clone)]
struct Rule {
    matcher: Matcher,
    action: RuleAction,
}

#[derive(Debug, Clone)]
enum Matcher {
    LiteralPath(PathBuf),
    Glob(Pattern),
}

impl Matcher {
    fn matches(&self, path: &Path) -> bool {
        match self {
            Matcher::LiteralPath(prefix) => path == prefix || path.starts_with(prefix),
            Matcher::Glob(pattern) => pattern.matches_path(path),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Profile {
    rules: Vec<Rule>,
    implicit_visible_ancestors: BTreeSet<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Action(RuleAction),
    ImplicitAncestor,
    Hidden,
}

impl Profile {
    pub fn parse(profile_src: &str, launch_cwd: &Path) -> Result<Self> {
        let cwd = normalize_abs(launch_cwd)
            .context("launch cwd for profile parsing must be an absolute normalized path")?;

        let mut rules = Vec::new();
        let mut implicit_visible_ancestors = BTreeSet::new();

        for (idx, line) in profile_src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let mut parts = trimmed.split_whitespace();
            let pattern_token = parts
                .next()
                .with_context(|| format!("line {} missing pattern", idx + 1))?;
            let action_token = parts
                .next()
                .with_context(|| format!("line {} missing action", idx + 1))?;
            if parts.next().is_some() {
                bail!("line {} has extra tokens", idx + 1);
            }

            let action = parse_action(action_token)
                .with_context(|| format!("line {} has invalid action", idx + 1))?;
            let matcher = parse_matcher(pattern_token, &cwd)
                .with_context(|| format!("line {} has invalid pattern", idx + 1))?;

            if action != RuleAction::Deny {
                gather_implicit_ancestors(&matcher, &mut implicit_visible_ancestors);
            }

            rules.push(Rule { matcher, action });
        }

        Ok(Self {
            rules,
            implicit_visible_ancestors,
        })
    }

    pub fn first_match_action(&self, abs_path: &Path) -> Option<RuleAction> {
        let normalized = normalize_abs(abs_path).ok()?;
        self.rules
            .iter()
            .find(|rule| rule.matcher.matches(&normalized))
            .map(|rule| rule.action)
    }

    pub fn visibility(&self, abs_path: &Path) -> Visibility {
        let Ok(normalized) = normalize_abs(abs_path) else {
            return Visibility::Hidden;
        };

        if let Some(action) = self.first_match_action(&normalized) {
            return Visibility::Action(action);
        }

        if self.implicit_visible_ancestors.contains(&normalized) {
            return Visibility::ImplicitAncestor;
        }

        Visibility::Hidden
    }
}

fn parse_action(token: &str) -> Result<RuleAction> {
    match token {
        "ro" => Ok(RuleAction::ReadOnly),
        "rw" => Ok(RuleAction::ReadWrite),
        "deny" => Ok(RuleAction::Deny),
        _ => bail!("action must be one of ro/rw/deny"),
    }
}

fn parse_matcher(token: &str, cwd: &Path) -> Result<Matcher> {
    let normalized = if token == "." {
        cwd.to_path_buf()
    } else {
        let path = Path::new(token);
        if path.is_absolute() {
            normalize_abs(path)
                .with_context(|| format!("invalid absolute path pattern: {token}"))?
        } else {
            normalize_abs(&cwd.join(path))
                .with_context(|| format!("invalid relative path pattern: {token}"))?
        }
    };

    let normalized_str = normalized
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("path pattern is not valid UTF-8"))?;

    if has_glob_syntax(normalized_str) {
        let pattern = Pattern::new(normalized_str)
            .with_context(|| format!("invalid glob pattern: {normalized_str}"))?;
        Ok(Matcher::Glob(pattern))
    } else {
        Ok(Matcher::LiteralPath(normalized))
    }
}

fn gather_implicit_ancestors(matcher: &Matcher, output: &mut BTreeSet<PathBuf>) {
    let mut fixed_path = match matcher {
        Matcher::LiteralPath(path) => path.clone(),
        Matcher::Glob(pattern) => fixed_prefix(pattern.as_str()),
    };

    while let Some(parent) = fixed_path.parent() {
        let parent = parent.to_path_buf();
        if parent.as_os_str().is_empty() {
            break;
        }
        if !output.insert(parent.clone()) {
            break;
        }
        fixed_path = parent;
    }
}

fn fixed_prefix(pattern: &str) -> PathBuf {
    let wildcard_idx = pattern.find(['*', '?', '[']).unwrap_or(pattern.len());
    let prefix = &pattern[..wildcard_idx];
    let trimmed = prefix.trim_end_matches('/');
    if trimmed.is_empty() {
        PathBuf::from("/")
    } else {
        PathBuf::from(trimmed)
    }
}

fn has_glob_syntax(value: &str) -> bool {
    value.contains('*') || value.contains('?') || value.contains('[')
}

fn normalize_abs(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("path must be absolute: {}", path.display());
    }

    let mut out = PathBuf::new();
    out.push("/");

    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(seg) => out.push(seg),
            Component::Prefix(_) => bail!("unsupported path prefix in {}", path.display()),
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Profile {
        Profile::parse(src, Path::new("/work")).expect("profile should parse")
    }

    #[test]
    fn first_match_wins() {
        let profile = parse(
            r#"
            /etc rw
            /etc ro
            "#,
        );
        assert_eq!(
            profile.first_match_action(Path::new("/etc/passwd")),
            Some(RuleAction::ReadWrite)
        );
    }

    #[test]
    fn unmatched_path_is_hidden() {
        let profile = parse(
            r#"
            /tmp rw
            "#,
        );
        assert_eq!(profile.visibility(Path::new("/opt")), Visibility::Hidden);
    }

    #[test]
    fn dot_expands_to_launch_cwd() {
        let profile = parse(
            r#"
            . rw
            "#,
        );
        assert_eq!(
            profile.first_match_action(Path::new("/work/foo")),
            Some(RuleAction::ReadWrite)
        );
    }

    #[test]
    fn parent_dir_is_implicitly_visible() {
        let profile = parse(
            r#"
            /foo/bar rw
            "#,
        );
        assert_eq!(
            profile.visibility(Path::new("/foo")),
            Visibility::ImplicitAncestor
        );
        assert_eq!(
            profile.first_match_action(Path::new("/foo/bar/baz")),
            Some(RuleAction::ReadWrite)
        );
    }

    #[test]
    fn deny_still_overrides_visibility_when_matched() {
        let profile = parse(
            r#"
            /foo/bar rw
            /foo deny
            "#,
        );
        assert_eq!(
            profile.visibility(Path::new("/foo")),
            Visibility::Action(RuleAction::Deny)
        );
    }
}
