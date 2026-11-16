//! Scriptlet injection: the trusted-function registry uBlock Origin
//! shaped. A scriptlet is a named behavior with arguments — parse the
//! filter, validate the name against the registry, inject before page
//! scripts run. The registry is ours; unknown names never execute.

/// The trusted scriptlets. Each entry: name, argument count, one-line
/// behavior. Adding a scriptlet means adding it here and implementing
/// it in the engine's injected-script runtime — the registry is the
/// gate.
pub const SCRIPTLETS: &[(&str, usize, &str)] = &[
    ("abort-on-property-read", 1, "throw on reads of the named property"),
    ("abort-on-property-write", 1, "throw on writes to the named property"),
    ("abort-current-inline-script", 1, "kill inline scripts matching a pattern"),
    ("set-constant", 2, "pin a property to a value"),
    ("remove-attr", 1, "strip an attribute from matched elements"),
    ("prevent-addEventListener", 1, "refuse listener registration for an event type"),
    ("noop-fetch", 0, "fetch resolves empty, never network"),
    ("json-prune", 2, "remove keys from JSON responses"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct InjectionRule {
    pub host: String,
    pub scriptlet: String,
    pub args: Vec<String>,
}

/// Parse a scriptlet filter line:
/// `example.com##script:abort-on-property-read(adPath)`.
/// Hosts may be comma-listed; the scriptlet name must exist in the
/// registry with the exact argument count or the rule is refused.
pub fn parse_filter(line: &str) -> Result<InjectionRule, String> {
    let line = line.trim();
    let (hosts, script) = line
        .split_once("##script:")
        .ok_or_else(|| format!("not a scriptlet rule: {line:?}"))?;

    let host = hosts
        .split(',')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if host.is_empty() {
        return Err("scriptlet rule without a host".into());
    }

    let open = script
        .find('(')
        .ok_or_else(|| format!("scriptlet without arguments: {script:?}"))?;
    let name = script[..open].trim().to_owned();
    let close = script.rfind(')').ok_or_else(|| format!("unterminated scriptlet: {script:?}"))?;
    if close < open {
        return Err(format!("unterminated scriptlet: {script:?}"));
    }
    let args: Vec<String> = script[open + 1..close]
        .split(',')
        .map(|arg| arg.trim().trim_matches('"').to_owned())
        .filter(|arg| !arg.is_empty())
        .collect();

    let (_, arity, _) = SCRIPTLETS
        .iter()
        .find(|(known, _, _)| *known == name)
        .ok_or_else(|| format!("untrusted scriptlet name: {name:?}"))?;
    if args.len() != *arity {
        return Err(format!(
            "scriptlet {name:?} takes {arity} args, got {}",
            args.len()
        ));
    }

    Ok(InjectionRule {
        host,
        scriptlet: name,
        args,
    })
}

/// Injection ordering: every scriptlet runs before any page script —
/// the whole point is to win the race with the ad's own boot code.
/// One rule per host wins (the most specific, last-registered).
pub fn order_for_page<'a>(rules: &'a [InjectionRule], host: &str) -> Vec<&'a InjectionRule> {
    rules
        .iter()
        .filter(|rule| host == rule.host || host.ends_with(&format!(".{}", rule.host)))
        .collect()
}
