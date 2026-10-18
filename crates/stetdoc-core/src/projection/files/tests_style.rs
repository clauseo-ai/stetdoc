//! The style of a new paragraph is Word's next style (law T8, design 6.4).

use super::style::{Previous, styles};
use super::testing::styles as table;

fn ids(previous: Previous, count: usize) -> Vec<Option<String>> {
    styles(count, &previous, &table())
        .into_iter()
        .map(|s| s.map(|s| s.id))
        .collect()
}

#[test]
fn after_a_heading_comes_body_text_and_then_more_body_text() {
    assert_eq!(
        ids(Previous::Follows(Some("Heading 1".into())), 3),
        vec![Some("BodyText".to_string()), None, None],
        "the first is body text, and the second and third are copies of it"
    );
}

#[test]
fn a_title_is_followed_by_a_subtitle_and_then_body_text() {
    assert_eq!(
        ids(Previous::Follows(Some("Title".into())), 4),
        vec![
            Some("Subtitle".to_string()),
            Some("BodyText".to_string()),
            None,
            None
        ]
    );
}

#[test]
fn a_style_whose_next_is_itself_is_the_copy_rule_which_keeps_list_levels_and_indents() {
    assert_eq!(
        ids(Previous::Follows(Some("List Paragraph".into())), 3),
        vec![None, None, None]
    );
    assert_eq!(
        ids(Previous::Follows(Some("Body Text".into())), 2),
        vec![None, None]
    );
}

#[test]
fn a_style_that_is_not_known_changes_nothing() {
    assert_eq!(
        ids(Previous::Follows(Some("Mystery".into())), 2),
        vec![None, None]
    );
    assert_eq!(ids(Previous::Follows(None), 2), vec![None, None]);
}

#[test]
fn a_run_that_follows_no_paragraph_is_modelled_on_the_one_it_precedes_and_the_chain_starts_second()
{
    assert_eq!(
        ids(Previous::Precedes(Some("Heading 1".into())), 3),
        vec![None, Some("BodyText".to_string()), None]
    );
    assert_eq!(ids(Previous::Precedes(None), 2), vec![None, None]);
}

#[test]
fn nothing_is_asked_for_an_empty_run() {
    assert!(ids(Previous::Follows(Some("Heading 1".into())), 0).is_empty());
}
