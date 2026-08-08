mod authorization;
mod workspace;

pub(crate) use authorization::authorize_main_window;
pub(crate) use workspace::{
    prepare_foundation_workspace, probe_workspace, validate_workspace_path_syntax,
};

const MAX_TECHNICAL_DETAIL_CHARS: usize = 512;

pub(crate) fn sanitize_technical_detail(value: &str) -> String {
    let mut used_utf16_units = 0;
    let normalized: String = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take_while(|character| {
            let next_units = character.len_utf16();
            if used_utf16_units + next_units > MAX_TECHNICAL_DETAIL_CHARS {
                false
            } else {
                used_utf16_units += next_units;
                true
            }
        })
        .collect();

    let without_paths = redact_path_tokens(&normalized);
    redact_token_after(&redact_openrouter_keys(&without_paths), "Bearer ")
}

fn redact_path_tokens(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            if token.contains(":\\") || token.contains(":/") || token.starts_with("\\\\") {
                "[PATH]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_openrouter_keys(value: &str) -> String {
    redact_token_after(value, "sk-or-")
}

fn redact_token_after(value: &str, prefix: &str) -> String {
    let mut remaining = value;
    let mut output = String::with_capacity(value.len());

    while let Some(index) = remaining.find(prefix) {
        let (before, token_and_after) = remaining.split_at(index);
        output.push_str(before);
        output.push_str("[REDACTED]");

        let token_value = &token_and_after[prefix.len()..];
        let token_end = token_value
            .char_indices()
            .find_map(|(offset, character)| character.is_whitespace().then_some(offset))
            .unwrap_or(token_value.len());
        remaining = &token_value[token_end..];
    }

    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::sanitize_technical_detail;

    #[test]
    fn technical_details_remove_control_characters_and_tokens() {
        let sanitized = sanitize_technical_detail(
            "request\nAuthorization: Bearer secret-canary and sk-or-private-canary at C:\\Users\\Example\\meeting.md",
        );

        assert!(!sanitized.contains('\n'));
        assert!(!sanitized.contains("secret-canary"));
        assert!(!sanitized.contains("sk-or-private-canary"));
        assert!(!sanitized.contains("Example"));
        assert!(sanitized.contains("[PATH]"));
        assert_eq!(sanitized.matches("[REDACTED]").count(), 2);
    }
}
