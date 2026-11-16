//! The accessibility inspector: the engine's a11y tree as searchable rows,
//! plus audit checks with a score.

use crate::inspector::Rect;
use serde_json::{json, Value};

/// One a11y tree node as a row. `contrast` is the computed text contrast
/// where the node has text, `None` otherwise; `heading_level` is set for
/// headings, because audit checks need both.
#[derive(Debug, Clone)]
pub struct A11yRow {
    pub node: u64,
    pub role: String,
    pub name: String,
    pub bounds: Rect,
    pub states: Vec<String>,
    pub contrast: Option<f32>,
    pub heading_level: Option<u8>,
}

/// WCAG 2.1 AA minimum contrast for body text. The audit reports the
/// number, not a pass/fail emoji.
pub const AA_MIN_CONTRAST: f32 = 4.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// One audit finding, with the node it points at so a click navigates the
/// tree to the offender.
#[derive(Debug, Clone)]
pub enum AuditCheck {
    ImageWithoutName { node: u64 },
    ButtonWithoutLabel { node: u64 },
    LowContrast { node: u64, ratio: f32 },
    HeadingOrder { node: u64, skipped_level: u8 },
    FocusableWithoutRole { node: u64 },
}

impl AuditCheck {
    pub fn node(&self) -> u64 {
        match self {
            AuditCheck::ImageWithoutName { node }
            | AuditCheck::ButtonWithoutLabel { node }
            | AuditCheck::LowContrast { node, .. }
            | AuditCheck::HeadingOrder { node, .. }
            | AuditCheck::FocusableWithoutRole { node } => *node,
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            AuditCheck::ImageWithoutName { .. } => "a11y.image_without_name",
            AuditCheck::ButtonWithoutLabel { .. } => "a11y.button_without_label",
            AuditCheck::LowContrast { .. } => "a11y.low_contrast",
            AuditCheck::HeadingOrder { .. } => "a11y.heading_order",
            AuditCheck::FocusableWithoutRole { .. } => "a11y.focusable_without_role",
        }
    }

    /// Contrast below 3:1 is an error (invisible to most users); the band
    /// from 3:1 to 4.5:1 is a warning. Everything else errors: a button
    /// without a name is unusable with a screen reader, full stop.
    pub fn severity(&self) -> Severity {
        match self {
            AuditCheck::LowContrast { ratio, .. } if *ratio >= 3.0 => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

/// The inspector: ingested rows, a live filter, and the audit state.
pub struct A11yInspector {
    pub rows: Vec<A11yRow>,
    pub filter: String,
    checks: Vec<AuditCheck>,
    checks_total: usize,
}

impl A11yInspector {
    pub fn new() -> A11yInspector {
        A11yInspector { rows: Vec::new(), filter: String::new(), checks: Vec::new(), checks_total: 0 }
    }

    /// Consumes a fresh tree from the engine and reruns the audit. Every
    /// audit point that could fire counts toward the total, so the score
    /// is a ratio of fired checks to possible checks, not to nodes.
    pub fn ingest(&mut self, rows: Vec<A11yRow>) {
        self.checks_total = 0;
        let mut checks = Vec::new();
        let mut prev_heading: Option<u8> = None;
        for row in &rows {
            let unnamed = row.name.trim().is_empty();
            if row.role == "img" {
                self.checks_total += 1;
                if unnamed {
                    checks.push(AuditCheck::ImageWithoutName { node: row.node });
                }
            }
            if row.role == "button" {
                self.checks_total += 1;
                if unnamed {
                    checks.push(AuditCheck::ButtonWithoutLabel { node: row.node });
                }
            }
            if let Some(ratio) = row.contrast {
                self.checks_total += 1;
                if ratio < AA_MIN_CONTRAST {
                    checks.push(AuditCheck::LowContrast { node: row.node, ratio });
                }
            }
            if let Some(level) = row.heading_level {
                self.checks_total += 1;
                if prev_heading.is_some_and(|prev| level > prev + 1) {
                    checks.push(AuditCheck::HeadingOrder { node: row.node, skipped_level: level });
                }
                prev_heading = Some(level);
            }
            if row.states.iter().any(|state| state == "focusable") && row.role == "generic" {
                self.checks_total += 1;
                checks.push(AuditCheck::FocusableWithoutRole { node: row.node });
            }
        }
        self.checks = checks;
        self.rows = rows;
    }

    /// Rows surviving the filter: role or name contains the text.
    pub fn visible(&self) -> Vec<&A11yRow> {
        let needle = self.filter.to_lowercase();
        self.rows
            .iter()
            .filter(|row| {
                needle.is_empty()
                    || row.role.to_lowercase().contains(&needle)
                    || row.name.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn checks(&self) -> &[AuditCheck] {
        &self.checks
    }

    /// Failed checks over executed checks. A perfect tree scores 0.0, not
    /// 1.0 — the score counts problems, and a page without problems has
    /// none to count.
    pub fn score(&self) -> f32 {
        if self.checks_total == 0 {
            return 0.0;
        }
        self.checks.len() as f32 / self.checks_total as f32
    }

    pub fn to_json(&self) -> Value {
        let rows: Vec<Value> = self
            .visible()
            .into_iter()
            .map(|row| {
                json!({
                    "node": row.node,
                    "role": row.role,
                    "name": row.name,
                    "bounds": [row.bounds.x, row.bounds.y, row.bounds.w, row.bounds.h],
                    "states": row.states,
                })
            })
            .collect();
        let checks: Vec<Value> = self
            .checks
            .iter()
            .map(|check| {
                json!({
                    "key": check.key(),
                    "node": check.node(),
                    "severity": match check.severity() { Severity::Error => "error", Severity::Warning => "warning" },
                })
            })
            .collect();
        json!({ "rows": rows, "checks": checks, "score": self.score() })
    }
}

impl Default for A11yInspector {
    fn default() -> Self {
        A11yInspector::new()
    }
}
