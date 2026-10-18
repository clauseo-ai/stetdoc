//! One finding as the checks produce it, before it has a part or a block.

use super::keys::{Key, unrenamed};
use super::report::sentence;
use crate::api::check::FindingCode;
use crate::xml::NodeId;

/// A finding of a Word rule, a package rule or the audit, or a schema violation, with the key
/// that says what is wrong and the node it was found at, when there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    /// What is wrong, independent of where.
    pub(crate) key: Key,
    /// The stable code.
    pub(crate) code: FindingCode,
    /// The element it was found at. `None` for a problem of a part as a whole.
    pub(crate) node: Option<NodeId>,
    /// A sentence that says what is wrong.
    pub(crate) message: String,
}

impl Found {
    /// A finding of a rule about the Word element `local`. Its key names the element as it was
    /// before the compiler renamed it (`unrenamed`), and its message names it as it is.
    pub(crate) fn element(code: FindingCode, local: &str, node: Option<NodeId>) -> Found {
        Found {
            key: Key::rule(code, unrenamed(local)),
            code,
            node,
            message: sentence(code, local),
        }
    }

    /// A finding of a rule about `subject`, in the words of [`sentence`].
    pub(crate) fn rule(code: FindingCode, subject: &str, node: Option<NodeId>) -> Found {
        Found {
            key: Key::rule(code, subject),
            code,
            node,
            message: sentence(code, subject),
        }
    }
}
