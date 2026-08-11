use shiki_config::Config;

/// One entry in the inline editor's `/`-menu — either one of `builtins()`
/// or a `[[snippets]]` entry from `config.toml` (`Config::snippets`, see
/// `shiki_config::config::SnippetConfig` for the on-disk shape).
#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub trigger: String,
    pub label: String,
    /// `{{title}}`/`{{date}}` are substituted the same way note templates
    /// are (`shiki_core::Template::render` — `App::apply_slash_command`
    /// reuses it directly). A literal `{{cursor}}` marks where the cursor
    /// should land after insertion; it's resolved separately and never
    /// ends up in the actually-inserted text. Omit it to leave the cursor
    /// at the end of the snippet, which is already correct for anything
    /// that's just one line with nothing to fill in (`h1`, `divider`).
    pub body: String,
}

fn builtin(trigger: &str, label: &str, body: &str) -> SlashCommand {
    SlashCommand {
        trigger: trigger.to_string(),
        label: label.to_string(),
        body: body.to_string(),
    }
}

/// The commands every install starts with — deliberately not persisted to
/// `config.toml` (unlike templates, which `ensure_defaults` writes to
/// disk): there's nothing to customize by hand-editing a file here unless
/// a user actually wants to, so nothing is written until they add their
/// own `[[snippets]]` entry.
pub fn builtins() -> Vec<SlashCommand> {
    vec![
        builtin("h1", "Heading 1", "# {{cursor}}"),
        builtin("h2", "Heading 2", "## {{cursor}}"),
        builtin("h3", "Heading 3", "### {{cursor}}"),
        builtin("bold", "Bold text", "**{{cursor}}**"),
        builtin("italic", "Italic text", "*{{cursor}}*"),
        builtin("code", "Code block", "```\n{{cursor}}\n```"),
        builtin("math", "Math block", "$$\n{{cursor}}\n$$"),
        builtin(
            "table",
            "Table",
            "| Column | Column |\n| --- | --- |\n| {{cursor}} |  |\n",
        ),
        builtin("check", "Checklist item", "- [ ] {{cursor}}"),
        builtin("quote", "Quote", "> {{cursor}}"),
        builtin("divider", "Divider", "---\n"),
        builtin("date", "Today's date", "{{date}}"),
        builtin("tags", "Tags line", "Tags: {{cursor}}"),
        builtin(
            "frontmatter",
            "YAML frontmatter block",
            "---\ntitle: {{title}}\ndate: {{date}}\ntags: []\n---\n{{cursor}}",
        ),
        builtin("bullet", "Bullet list item", "- {{cursor}}"),
        builtin("numbered", "Numbered list item", "1. {{cursor}}"),
        builtin("link", "Link", "[{{cursor}}]()"),
        builtin("image", "Image", "![{{cursor}}]()"),
        builtin("note", "Note callout", "> **Note:** {{cursor}}"),
        builtin("warning", "Warning callout", "> **Warning:** {{cursor}}"),
        builtin(
            "details",
            "Collapsible section",
            "<details>\n<summary>{{cursor}}</summary>\n\n</details>\n",
        ),
    ]
}

/// The full `/`-menu list: built-ins, with any `config.toml`
/// `[snippets.<trigger>]` entry of the same trigger (case-insensitive)
/// overriding it in place — so a user can redefine `code` or `h1` just as
/// easily as add a brand new command — otherwise appended after the
/// built-ins.
///
/// Iterates `config.snippets` in sorted-by-trigger order, not the
/// `HashMap`'s own (randomized per process) order — matters specifically
/// when two custom entries collide case-insensitively with *each other*
/// (e.g. both `[snippets.H1]` and `[snippets.h1]`, a real "duplicate
/// command by mistake" someone could actually type): without a fixed
/// order, which one ends up applied would silently change between runs of
/// the exact same config. Sorting doesn't make the collision *correct* —
/// `shiki doctor` reports it — but at least the outcome is reproducible.
pub fn all_commands(config: &Config) -> Vec<SlashCommand> {
    let mut commands = builtins();
    let mut custom_entries: Vec<(&String, &shiki_config::config::SnippetConfig)> =
        config.snippets.iter().collect();
    // Secondary sort by the exact (case-sensitive) trigger, not just its
    // lowercased form: two entries that collide case-insensitively (the
    // scenario this whole ordering exists for) compare *equal* on the
    // lowercased key alone, and a stable sort leaves equal elements in
    // whatever order the `HashMap` happened to hand them over in — which
    // is exactly the non-determinism being fixed. The plain string tie-
    // breaker is fully deterministic regardless of hash randomization.
    custom_entries.sort_by(|(a, _), (b, _)| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    });
    for (trigger, custom) in custom_entries {
        let entry = SlashCommand {
            trigger: trigger.clone(),
            label: custom.label.clone().unwrap_or_else(|| trigger.clone()),
            body: custom.body.clone(),
        };
        match commands
            .iter_mut()
            .find(|c| c.trigger.eq_ignore_ascii_case(&entry.trigger))
        {
            Some(existing) => *existing = entry,
            None => commands.push(entry),
        }
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiki_config::config::SnippetConfig;

    #[test]
    fn custom_snippet_is_appended() {
        let mut config = Config::default();
        config.snippets.insert(
            "callout".to_string(),
            SnippetConfig {
                label: Some("Callout".to_string()),
                body: "> {{cursor}}".to_string(),
            },
        );
        let commands = all_commands(&config);
        assert_eq!(commands.len(), builtins().len() + 1);
        assert!(commands.iter().any(|c| c.trigger == "callout"));
    }

    #[test]
    fn custom_snippet_overrides_a_builtin_case_insensitively() {
        let mut config = Config::default();
        config.snippets.insert(
            "H1".to_string(),
            SnippetConfig {
                label: None,
                body: "# custom {{cursor}}".to_string(),
            },
        );
        let commands = all_commands(&config);
        assert_eq!(commands.len(), builtins().len());
        let h1 = commands.iter().find(|c| c.trigger == "H1").unwrap();
        assert_eq!(h1.body, "# custom {{cursor}}");
        assert_eq!(h1.label, "H1");
    }

    /// `[snippets.H1]` and `[snippets.h1]` in the *same* config.toml — a
    /// real "typo'd a duplicate command" scenario, since TOML itself only
    /// rejects an exact duplicate key, not a same-letters-different-case
    /// one. Before sorting `config.snippets` deterministically, which body
    /// ended up applied depended on `HashMap`'s own randomized iteration
    /// order — this pins down that it's now always the same one,
    /// regardless of insertion order.
    #[test]
    fn colliding_custom_triggers_resolve_deterministically() {
        let mut config = Config::default();
        config.snippets.insert(
            "H1".to_string(),
            SnippetConfig {
                label: None,
                body: "uppercase wins?".to_string(),
            },
        );
        config.snippets.insert(
            "h1".to_string(),
            SnippetConfig {
                label: None,
                body: "lowercase wins?".to_string(),
            },
        );
        // Only one survives (both collapse onto the same builtin slot).
        let commands = all_commands(&config);
        assert_eq!(commands.len(), builtins().len());
        let winner = commands
            .iter()
            .find(|c| c.trigger.eq_ignore_ascii_case("h1"))
            .unwrap();
        // The exact winner is an implementation detail (whichever sorts
        // last); what matters is that it's the *same* one every time.
        assert_eq!(winner.body, "lowercase wins?");
        assert_eq!(winner.trigger, "h1");
    }
}
