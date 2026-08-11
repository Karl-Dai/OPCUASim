pub mod browse_panel;
pub mod connection_tree;
pub mod data_table;
pub mod events_panel;
pub mod history_tab;
pub mod log_panel;
pub mod toolbar;
pub mod value_panel;

pub fn quality_color(q: &str) -> egui::Color32 {
    use opcuaegui_shared::theme;
    if q.is_empty() {
        theme::STATUS_IDLE()
    } else if q.starts_with("Good") {
        theme::STATUS_OK()
    } else if q.starts_with("Bad") || q.contains("Error") {
        theme::STATUS_BAD()
    } else if q.starts_with("Uncertain") {
        theme::STATUS_WARN()
    } else {
        theme::TEXT_MUTED()
    }
}

pub fn format_hms(ts: Option<&str>) -> String {
    let Some(raw) = ts else {
        return String::from("—");
    };
    if raw.is_empty() {
        return String::from("—");
    }
    if raw.len() >= 19 {
        raw[11..19].to_string()
    } else {
        raw.to_string()
    }
}

/// Detect complex values from the core `variant_to_display_string` output.
/// Arrays start with `[`; Structures/ExtensionObjects produce long opaque Display text.
pub fn is_complex_value(v: &str) -> bool {
    v.starts_with('[') || v.len() > 50
}

/// Truncate at a UTF-8 char boundary — `&s[..max]` would panic on multi-byte CJK.
pub fn truncate_safe(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        match s.char_indices().nth(max).map(|(i, _)| i) {
            Some(i) => &s[..i],
            None => s,
        }
    }
}

/// Render `variant_to_tree` output as nested `CollapsingHeader`s.
/// `id_salt(i)` scopes duplicate sibling names (e.g. array indices `[0]`) to unique ids.
pub fn show_variant_tree(ui: &mut egui::Ui, nodes: &[opcuasim_core::values::TreeNode]) {
    use opcuaegui_shared::theme;
    for (i, node) in nodes.iter().enumerate() {
        if node.children.is_empty() {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&node.name)
                        .color(theme::TEXT_MUTED())
                        .monospace(),
                );
                ui.label(":");
                ui.label(
                    egui::RichText::new(&node.value)
                        .color(theme::TEXT_PRIMARY())
                        .monospace(),
                );
            });
        } else {
            egui::CollapsingHeader::new(node.name.as_str())
                .id_salt(i)
                .default_open(false)
                .show(ui, |ui| {
                    if !node.value.is_empty() {
                        ui.label(
                            egui::RichText::new(&node.value)
                                .small()
                                .color(theme::TEXT_MUTED())
                                .monospace(),
                        );
                    }
                    show_variant_tree(ui, &node.children);
                });
        }
    }
}
