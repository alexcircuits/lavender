//! Persistent daily request budget governor for `TomTom` traffic flow requests.

#![allow(clippy::collapsible_if, clippy::doc_markdown, clippy::must_use_candidate)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::tiles::utc_day_key;

/// Persisted daily budget record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetRecord {
    /// Date key in format `"YYYY-MM-DD"` (UTC).
    pub date: String,
    /// Request count accumulated today.
    pub count: u64,
}

/// Governor tracking daily upstream requests against a configured budget limit.
#[derive(Debug)]
pub struct BudgetGovernor {
    file_path: PathBuf,
    record: BudgetRecord,
}

impl BudgetGovernor {
    /// Load existing budget from disk or initialize fresh for today.
    #[must_use]
    pub fn load_or_init(budget_file: PathBuf, now_s: i64) -> Self {
        let today = utc_day_key(now_s);

        let record = if budget_file.is_file() {
            match fs::read_to_string(&budget_file) {
                Ok(content) => match serde_json::from_str::<BudgetRecord>(&content) {
                    Ok(mut rec) => {
                        if rec.date != today {
                            rec.date = today;
                            rec.count = 0;
                        }
                        rec
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse budget file {}: {e}",
                            budget_file.display()
                        );
                        BudgetRecord { date: today, count: 0 }
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read budget file {}: {e}", budget_file.display());
                    BudgetRecord { date: today, count: 0 }
                }
            }
        } else {
            BudgetRecord { date: today, count: 0 }
        };

        Self { file_path: budget_file, record }
    }

    /// Read the current normalized budget record (rolling over to today if UTC date changed).
    pub fn current(&mut self, now_s: i64) -> BudgetRecord {
        let today = utc_day_key(now_s);
        if self.record.date != today {
            self.record.date = today;
            self.record.count = 0;
            self.persist();
        }
        self.record.clone()
    }

    /// Check whether the budget has reached or exceeded `limit`.
    pub fn is_over_budget(&mut self, now_s: i64, limit: u64) -> bool {
        if limit == 0 {
            return false;
        }
        let current = self.current(now_s);
        current.count >= limit
    }

    /// Record one upstream request attempt against today's budget and persist.
    pub fn record_fetch(&mut self, now_s: i64) {
        let today = utc_day_key(now_s);
        if self.record.date != today {
            self.record.date = today;
            self.record.count = 0;
        }
        self.record.count = self.record.count.saturating_add(1);
        self.persist();
    }

    /// Persist the current budget record to disk atomically.
    fn persist(&self) {
        if let Some(parent) = self.file_path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            tracing::warn!("Failed to create budget dir {}: {e}", parent.display());
            return;
        }

        let tmp_path = self.file_path.with_extension("tmp");
        let Ok(json) = serde_json::to_string_pretty(&self.record) else {
            return;
        };

        if fs::write(&tmp_path, json.as_bytes()).is_ok() {
            let _ = fs::rename(&tmp_path, &self.file_path);
        }
    }

    /// Return path to budget file (for testing inspection).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.file_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_survives_restarts() {
        let temp_dir =
            std::env::temp_dir().join(format!("lavender-budget-test-{}", std::process::id()));
        let budget_path = temp_dir.join("budget.json");
        let now = 1_774_000_000; // Fixed timestamp

        // Session 1: init and record fetches
        {
            let mut gov = BudgetGovernor::load_or_init(budget_path.clone(), now);
            assert_eq!(gov.current(now).count, 0);
            assert!(!gov.is_over_budget(now, 10));

            gov.record_fetch(now);
            gov.record_fetch(now);
            gov.record_fetch(now);
            assert_eq!(gov.current(now).count, 3);
        }

        // Session 2 (restart): re-load from disk and verify count persists!
        {
            let mut gov = BudgetGovernor::load_or_init(budget_path.clone(), now);
            assert_eq!(gov.current(now).count, 3, "count must survive restart");
            assert!(!gov.is_over_budget(now, 5));
            assert!(gov.is_over_budget(now, 3));

            gov.record_fetch(now);
            assert_eq!(gov.current(now).count, 4);
        }

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_budget_utc_rollover() {
        let temp_dir =
            std::env::temp_dir().join(format!("lavender-budget-rollover-{}", std::process::id()));
        let budget_path = temp_dir.join("budget.json");
        let day1 = 1_774_000_000; // 2026-03-20
        let day2 = day1 + 86_400; // 2026-03-21

        {
            let mut gov = BudgetGovernor::load_or_init(budget_path.clone(), day1);
            gov.record_fetch(day1);
            gov.record_fetch(day1);
            assert_eq!(gov.current(day1).count, 2);
            assert_eq!(gov.current(day1).date, "2026-03-20");

            // Advance to day 2
            assert_eq!(gov.current(day2).count, 0, "must roll over to 0 on new day");
            assert_eq!(gov.current(day2).date, "2026-03-21");
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
