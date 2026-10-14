//! Deterministic retention arithmetic with injected time, sizes and live IDs.
use crate::config::invalid;
use std::{collections::BTreeSet, io};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    /// Allocated bytes measured by the known-layout store, not file length.
    pub bytes: u64,
    /// Newest modification among the session's known records and directories.
    pub newest_unix_s: u64,
    /// A recognized remainder already moved into this cleaner's quarantine.
    pub pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub before_bytes: u64,
    pub remove: Vec<String>,
    pub remove_bytes: u64,
    pub remaining_bytes: u64,
    pub refused: Vec<String>,
}

pub fn session_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn make(
    sessions: &[Session],
    live: &BTreeSet<String>,
    now_unix_s: u64,
    max_bytes: u64,
    max_age_seconds: u64,
) -> io::Result<Plan> {
    if max_bytes == 0 || max_age_seconds == 0 || sessions.len() > 16_384 {
        return Err(invalid("invalid retention limits or too many sessions"));
    }
    let mut ids = BTreeSet::new();
    let mut total = 0_u64;
    for item in sessions {
        if !session_id(&item.id) || !ids.insert(&item.id) {
            return Err(invalid("invalid or duplicate session identity"));
        }
        total = total
            .checked_add(item.bytes)
            .ok_or_else(|| invalid("record size overflow"))?;
    }
    let mut oldest: Vec<_> = sessions.iter().collect();
    oldest.sort_by(|a, b| (a.newest_unix_s, &a.id).cmp(&(b.newest_unix_s, &b.id)));
    let mut plan = Plan {
        before_bytes: total,
        remove: Vec::new(),
        remove_bytes: 0,
        remaining_bytes: total,
        refused: Vec::new(),
    };
    for item in oldest {
        if live.contains(&item.id) {
            continue;
        }
        if item.newest_unix_s > now_unix_s {
            plan.refused
                .push(format!("{}: modification time is in the future", item.id));
            continue;
        }
        let too_old = now_unix_s - item.newest_unix_s > max_age_seconds;
        if item.pending || too_old || plan.remaining_bytes > max_bytes {
            plan.remove.push(item.id.clone());
            plan.remove_bytes += item.bytes;
            plan.remaining_bytes -= item.bytes;
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(n: u32, bytes: u64, time: u64) -> Session {
        Session {
            id: format!("{n:032x}"),
            bytes,
            newest_unix_s: time,
            pending: false,
        }
    }
    fn ids(items: &[Session]) -> Vec<String> {
        items.iter().map(|item| item.id.clone()).collect()
    }

    #[test]
    fn size_removes_oldest_whole_sessions_until_within_bound() {
        let items = vec![item(3, 40, 300), item(1, 50, 100), item(2, 50, 200)];
        let plan = make(&items, &BTreeSet::new(), 400, 90, 1000).unwrap();
        assert_eq!(plan.remove, vec![items[1].id.clone()]);
        assert_eq!(
            (plan.before_bytes, plan.remove_bytes, plan.remaining_bytes),
            (140, 50, 90)
        );
    }
    #[test]
    fn age_is_strict_and_independent_from_size() {
        let items = vec![item(1, 10, 899), item(2, 10, 900), item(3, 10, 901)];
        let plan = make(&items, &BTreeSet::new(), 1000, 1000, 100).unwrap();
        assert_eq!(plan.remove, vec![items[0].id.clone()]);
    }
    #[test]
    fn live_session_is_kept_even_if_alone_older_and_larger_than_both_limits() {
        let items = vec![item(1, 1000, 1), item(2, 10, 900)];
        let live = BTreeSet::from([items[0].id.clone()]);
        let plan = make(&items, &live, 1000, 100, 10).unwrap();
        assert_eq!(plan.remove, vec![items[1].id.clone()]);
        assert_eq!(plan.remaining_bytes, 1000);
    }
    #[test]
    fn tied_times_use_stable_names_and_both_bounds_apply() {
        let items = vec![
            item(3, 20, 10),
            item(2, 20, 10),
            item(1, 20, 10),
            item(4, 90, 99),
        ];
        let plan = make(&items, &BTreeSet::new(), 100, 80, 10).unwrap();
        assert_eq!(
            plan.remove,
            ids(&[
                items[2].clone(),
                items[1].clone(),
                items[0].clone(),
                items[3].clone()
            ])
        );
        assert_eq!(plan.remaining_bytes, 0);
    }
    #[test]
    fn clock_reversal_preserves_future_records_and_reports_pressure() {
        let items = vec![item(1, 10, 1001), item(2, 10, 1000)];
        let plan = make(&items, &BTreeSet::new(), 1000, 5, 1).unwrap();
        assert_eq!(plan.remove, vec![items[1].id.clone()]);
        assert_eq!(plan.remaining_bytes, 10);
        assert_eq!(plan.refused.len(), 1);
    }
    #[test]
    fn duplicates_bad_names_and_overflow_are_errors_not_empty_success() {
        let item = item(1, u64::MAX, 1);
        assert!(make(&[item.clone(), item.clone()], &BTreeSet::new(), 10, 1, 1).is_err());
        let mut other = item.clone();
        other.id = format!("{:032x}", 2);
        assert!(make(&[item, other.clone()], &BTreeSet::new(), 10, 1, 1).is_err());
        other.id = "../../memory".into();
        assert!(make(&[other], &BTreeSet::new(), 10, 1, 1).is_err());
    }

    #[test]
    fn recent_quarantine_is_resumed_but_never_while_protected() {
        let mut pending = item(1, 1, 1000);
        pending.pending = true;
        let plan = make(
            std::slice::from_ref(&pending),
            &BTreeSet::new(),
            1000,
            100,
            100,
        )
        .unwrap();
        assert_eq!(plan.remove, vec![pending.id.clone()]);
        let plan = make(
            std::slice::from_ref(&pending),
            &BTreeSet::from([pending.id.clone()]),
            1000,
            100,
            100,
        )
        .unwrap();
        assert!(plan.remove.is_empty());
    }
}
