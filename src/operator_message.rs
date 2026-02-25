pub fn render_operator_message(
    what_happened: &str,
    why_likely: &str,
    do_this_now: &str,
) -> Vec<String> {
    vec![
        format!("  what_happened: {}", what_happened),
        format!("  why_likely: {}", why_likely),
        format!("  do_this_now: {}", do_this_now),
    ]
}
