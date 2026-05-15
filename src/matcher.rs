use crate::config::{Config, Rule};
use crate::detector::ActiveApp;
use regex::Regex;
use std::collections::HashMap;

/// Compiled, ready-to-evaluate version of a [`Rule`].
struct CompiledRule {
    rule: Rule,
    title_regex: Option<Regex>,
}

/// Holds all compiled rules so we don't re-parse regex on every poll.
pub struct Matcher {
    rules: Vec<CompiledRule>,
}

/// Resolved presence text for the current active app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPresence {
    pub rule_name: String,
    pub details: String,
    pub state: String,
    pub large_image: String,
    pub large_text: String,
    pub small_image: String,
    pub small_text: String,
    pub show_timer: bool,
}

impl Matcher {
    pub fn from_config(cfg: &Config) -> Self {
        let rules = cfg
            .rules
            .iter()
            .cloned()
            .map(|rule| {
                let title_regex = rule
                    .title_regex
                    .as_deref()
                    .and_then(|src| match Regex::new(src) {
                        Ok(r) => Some(r),
                        Err(e) => {
                            log::warn!(
                                "Invalid regex on rule '{}': {} ({})",
                                rule.name,
                                src,
                                e
                            );
                            None
                        }
                    });
                CompiledRule { rule, title_regex }
            })
            .collect();
        Self { rules }
    }

    /// Find the highest-priority rule matching the active app and render its
    /// presence template. Returns `None` if no rule matched.
    pub fn match_app(&self, app: &ActiveApp) -> Option<ResolvedPresence> {
        let mut best: Option<(&CompiledRule, HashMap<String, String>)> = None;

        for compiled in &self.rules {
            if let Some(captures) = evaluate(compiled, app) {
                let better = match &best {
                    None => true,
                    Some((current, _)) => compiled.rule.priority > current.rule.priority,
                };
                if better {
                    best = Some((compiled, captures));
                }
            }
        }

        let (compiled, captures) = best?;
        Some(render(&compiled.rule, app, &captures))
    }

    /// Resolve the configured idle presence (used when no rule matches).
    pub fn idle_presence(cfg: &Config) -> Option<ResolvedPresence> {
        if !cfg.idle.enabled {
            return None;
        }
        Some(ResolvedPresence {
            rule_name: "idle".to_string(),
            details: cfg.idle.details.clone(),
            state: cfg.idle.state.clone(),
            large_image: cfg.idle.large_image.clone(),
            large_text: cfg.idle.large_text.clone(),
            small_image: String::new(),
            small_text: String::new(),
            show_timer: false,
        })
    }
}

fn evaluate(compiled: &CompiledRule, app: &ActiveApp) -> Option<HashMap<String, String>> {
    let rule = &compiled.rule;
    let mut captures: HashMap<String, String> = HashMap::new();

    // Process must match if specified (case-insensitive substring on lowered process name).
    if let Some(needle) = rule.process.as_deref() {
        let needle = needle.to_lowercase();
        if !app.process.contains(&needle) {
            return None;
        }
    }

    // Title regex must match if specified, and exposes named captures as variables.
    if let Some(re) = &compiled.title_regex {
        let caps = re.captures(&app.title)?;
        for name in re.capture_names().flatten() {
            if let Some(m) = caps.name(name) {
                captures.insert(name.to_string(), m.as_str().trim().to_string());
            }
        }
    }

    // If neither process nor regex was specified, this rule is a fallback and
    // should not match arbitrary apps. Require at least one selector.
    if rule.process.is_none() && rule.title_regex.is_none() {
        return None;
    }

    Some(captures)
}

fn render(rule: &Rule, app: &ActiveApp, captures: &HashMap<String, String>) -> ResolvedPresence {
    ResolvedPresence {
        rule_name: rule.name.clone(),
        details: render_template(&rule.details, app, captures),
        state: render_template(&rule.state, app, captures),
        large_image: rule.large_image.clone(),
        large_text: render_template(&rule.large_text, app, captures),
        small_image: rule.small_image.clone(),
        small_text: render_template(&rule.small_text, app, captures),
        show_timer: rule.show_timer,
    }
}

/// Substitute `{key}` placeholders with values. Built-in keys: `process`, `title`.
/// Anything captured by a named group in `title_regex` is also available.
fn render_template(template: &str, app: &ActiveApp, captures: &HashMap<String, String>) -> String {
    if template.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut key = String::new();
            let mut closed = false;
            while let Some(&c) = chars.peek() {
                chars.next();
                if c == '}' {
                    closed = true;
                    break;
                }
                key.push(c);
            }
            if !closed {
                out.push('{');
                out.push_str(&key);
                continue;
            }
            let value = match key.as_str() {
                "process" => app.process.clone(),
                "title" => app.title.clone(),
                other => captures.get(other).cloned().unwrap_or_default(),
            };
            out.push_str(&value);
        } else {
            out.push(ch);
        }
    }
    out
}

/// Discord enforces text length limits on Rich Presence fields. Trim safely.
pub fn clamp_for_discord(s: &str) -> String {
    const MAX_BYTES: usize = 128;
    if s.len() <= MAX_BYTES {
        return s.to_string();
    }
    let mut end = MAX_BYTES;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    s[..end].trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rule;

    fn rule(name: &str, process: Option<&str>, regex: Option<&str>, priority: i32) -> Rule {
        Rule {
            name: name.into(),
            process: process.map(|s| s.into()),
            title_regex: regex.map(|s| s.into()),
            priority,
            details: "{title}".into(),
            state: String::new(),
            large_image: String::new(),
            large_text: String::new(),
            small_image: String::new(),
            small_text: String::new(),
            show_timer: true,
        }
    }

    fn matcher_from(rules: Vec<Rule>) -> Matcher {
        let cfg = Config {
            general: crate::config::General {
                client_id: String::new(),
                poll_interval_secs: 5,
                reset_timer_on_switch: true,
                start_minimized: true,
            },
            idle: Default::default(),
            rules,
        };
        Matcher::from_config(&cfg)
    }

    #[test]
    fn higher_priority_rule_wins() {
        let m = matcher_from(vec![
            rule("low", Some("code"), None, 1),
            rule("high", Some("code"), None, 10),
        ]);
        let app = ActiveApp {
            process: "code".into(),
            title: "main.rs - my-project - Visual Studio Code".into(),
        };
        let resolved = m.match_app(&app).expect("a rule should match");
        assert_eq!(resolved.rule_name, "high");
    }

    #[test]
    fn regex_named_captures_are_exposed() {
        let mut r = rule(
            "ssh",
            None,
            Some(r"(?i)ssh\s+(?P<host>[\w\.\-@]+)"),
            5,
        );
        r.details = "→ {host}".into();
        let m = matcher_from(vec![r]);
        let app = ActiveApp {
            process: "warp".into(),
            title: "ssh root@10.0.0.1".into(),
        };
        let resolved = m.match_app(&app).expect("ssh rule should match");
        assert_eq!(resolved.details, "→ root@10.0.0.1");
    }

    #[test]
    fn rule_without_selectors_never_matches() {
        let m = matcher_from(vec![rule("ghost", None, None, 100)]);
        let app = ActiveApp {
            process: "code".into(),
            title: "anything".into(),
        };
        assert!(m.match_app(&app).is_none());
    }

    #[test]
    fn template_uses_builtin_keys() {
        let captures = HashMap::new();
        let app = ActiveApp {
            process: "warp".into(),
            title: "ssh prod".into(),
        };
        assert_eq!(
            render_template("{process}: {title}", &app, &captures),
            "warp: ssh prod"
        );
    }

    #[test]
    fn clamp_handles_unicode_boundary() {
        let s = "a".repeat(120) + "🚀🚀🚀🚀";
        let clamped = clamp_for_discord(&s);
        assert!(clamped.len() <= 128);
    }
}
