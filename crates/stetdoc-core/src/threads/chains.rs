//! The chains of parents: which comment is each one's root, and where the loops are. One walk, linear in
//! the number of comments, whatever the shape: a chain of a hundred thousand replies and a ring of a
//! million comments both finish at once.

/// What the chains of parents come to.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Chains {
    /// For each comment, the comment at the end of its chain of parents.
    pub(super) root_of: Vec<usize>,
    /// The parents with each loop cut at its member that comes first in the file, which becomes a root.
    pub(super) parent: Vec<Option<usize>>,
    /// The member of each loop that comes first in the file, in the order the walk found them. A comment
    /// that names itself as parent is a loop of one.
    pub(super) loops: Vec<usize>,
}

/// Where a comment is in the walk.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Seen {
    /// Not reached yet.
    No,
    /// On the path being walked now.
    OnPath,
    /// Its root is known.
    Done,
}

/// Follows every chain of `raw` once. Each comment is put on the path at most once and finished at most once,
/// so the work is proportional to the number of comments.
pub(super) fn analyse(raw: &[Option<usize>]) -> Chains {
    let n = raw.len();
    let mut parent = raw.to_vec();
    let mut root_of = vec![0usize; n];
    let mut seen = vec![Seen::No; n];
    let mut loops = Vec::new();
    let mut path: Vec<usize> = Vec::new();
    for start in 0..n {
        if seen.get(start) != Some(&Seen::No) {
            continue;
        }
        path.clear();
        let mut at = start;
        let root = loop {
            match seen.get(at).copied() {
                Some(Seen::Done) => break root_of.get(at).copied().unwrap_or(at),
                Some(Seen::OnPath) => {
                    // `at` closes a loop: its members are the path from `at` on.
                    let from = path.iter().position(|m| *m == at).unwrap_or(0);
                    let first = path
                        .get(from..)
                        .and_then(|members| members.iter().min().copied())
                        .unwrap_or(at);
                    if let Some(slot) = parent.get_mut(first) {
                        *slot = None;
                    }
                    loops.push(first);
                    break first;
                }
                Some(Seen::No) => {
                    if let Some(slot) = seen.get_mut(at) {
                        *slot = Seen::OnPath;
                    }
                    path.push(at);
                    match raw.get(at).copied().flatten() {
                        Some(p) if p < n => at = p,
                        _ => break at,
                    }
                }
                None => break at,
            }
        };
        for m in &path {
            if let Some(slot) = root_of.get_mut(*m) {
                *slot = root;
            }
            if let Some(slot) = seen.get_mut(*m) {
                *slot = Seen::Done;
            }
        }
    }
    Chains {
        root_of,
        parent,
        loops,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    /// Runs `f` on another thread and fails if it does not finish: a quadratic walk is a hang.
    fn finishes<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("the walk did not finish in 20 seconds"))
    }

    #[test]
    fn roots_and_replies_find_their_roots() {
        // 0 is a root, 1 and 2 reply to it, 3 replies to 2, 4 is a root.
        let c = analyse(&[None, Some(0), Some(0), Some(2), None]);
        assert_eq!(c.root_of, vec![0, 0, 0, 0, 4]);
        assert!(c.loops.is_empty());
        assert_eq!(c.parent, vec![None, Some(0), Some(0), Some(2), None]);
    }

    #[test]
    fn a_loop_is_cut_at_its_first_member_and_what_leads_into_it_follows() {
        // 1 and 2 name each other, 0 hangs below 2, and 3 names itself.
        let c = analyse(&[Some(2), Some(2), Some(1), Some(3)]);
        assert_eq!(c.loops, vec![1, 3]);
        assert_eq!(c.parent, vec![Some(2), None, Some(1), None]);
        assert_eq!(c.root_of, vec![1, 1, 1, 3]);
    }

    #[test]
    fn a_chain_of_a_million_and_a_ring_of_a_million_are_walked_at_once() {
        let n: usize = 1_000_000;
        let chain = finishes(move || {
            let raw: Vec<Option<usize>> = (0..n).map(|i| i.checked_sub(1)).collect();
            analyse(&raw)
        });
        assert_eq!(chain.root_of.last().copied(), Some(0));
        assert!(chain.loops.is_empty());
        let backwards = finishes(move || {
            let raw: Vec<Option<usize>> = (0..n).map(|i| (i + 1 < n).then_some(i + 1)).collect();
            analyse(&raw)
        });
        assert_eq!(backwards.root_of.first().copied(), Some(n - 1));
        let ring = finishes(move || {
            let raw: Vec<Option<usize>> = (0..n).map(|i| Some((i + 1) % n)).collect();
            analyse(&raw)
        });
        assert_eq!(ring.loops, vec![0]);
        assert_eq!(ring.parent.first().copied(), Some(None));
        assert!(ring.root_of.iter().all(|r| *r == 0));
    }
}
