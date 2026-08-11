use crate::events::{EventItemDto, UiCommand};
use crate::model::{AppModel, EventsPanelState};
use crate::runtime::BackendHandle;

pub fn show(ui: &mut egui::Ui, model: &mut AppModel, backend: &BackendHandle) {
    // Actions accumulated during the UI frame and dispatched after the
    // state borrow is released.
    let mut subscribe_now: Option<(String, String)> = None;
    let mut unsubscribe_now: Option<String> = None;
    let mut clear_now: Option<String> = None;
    {
        let state = &mut model.events;
        let connections = model.connections.clone();
        let selected_conn = model.selected_conn.clone();

        if state.selected_conn.is_none() {
            state.selected_conn = selected_conn.clone();
        }

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("🔔 事件订阅")
                    .strong()
                    .color(opcuaegui_shared::theme::TEXT_PRIMARY()),
            );
            ui.separator();

            ui.label("连接:");
            let conn_label = state
                .selected_conn
                .as_ref()
                .and_then(|sel| connections.iter().find(|c| &c.id == sel))
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "未选择".to_string());

            egui::ComboBox::from_id_salt("events_conn_combo")
                .selected_text(conn_label)
                .width(180.0)
                .show_ui(ui, |ui| {
                    for conn in &connections {
                        let is_sel = state.selected_conn.as_deref() == Some(&conn.id);
                        if ui.selectable_label(is_sel, &conn.name).clicked() {
                            state.selected_conn = Some(conn.id.clone());
                            state.items.clear();
                            state.subscribed = false;
                            state.pending_subscribe_req = None;
                        }
                    }
                });

            ui.label("源节点:");
            let source_edit =
                egui::TextEdit::singleline(&mut state.source_node_id).desired_width(160.0);
            ui.add(source_edit);

            let has_conn = state.selected_conn.is_some();
            let in_flight = state.pending_subscribe_req.is_some();
            let can_subscribe =
                has_conn && !state.source_node_id.is_empty() && !state.subscribed && !in_flight;
            let sub_label = if in_flight {
                "⏳ 订阅中…"
            } else if state.subscribed {
                "✓ 已订阅"
            } else {
                "📡 订阅"
            };
            let sub_resp = ui.add_enabled(can_subscribe, egui::Button::new(sub_label));
            if sub_resp.clicked() {
                if let Some(ref conn_id) = state.selected_conn {
                    subscribe_now = Some((conn_id.clone(), state.source_node_id.clone()));
                    state.pending_subscribe_req = Some(0);
                }
            }

            let can_unsub = state.subscribed && !in_flight;
            let unsub_resp = ui.add_enabled(can_unsub, egui::Button::new("⛔ 取消"));
            if unsub_resp.clicked() {
                if let Some(ref conn_id) = state.selected_conn {
                    unsubscribe_now = Some(conn_id.clone());
                    state.subscribed = false;
                }
            }

            let clear_resp = ui.add_enabled(!state.items.is_empty(), egui::Button::new("🗑 清空"));
            if clear_resp.clicked() {
                if let Some(ref conn_id) = state.selected_conn {
                    clear_now = Some(conn_id.clone());
                    state.items.clear();
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("共 {} 条", state.items.len()))
                        .small()
                        .color(opcuaegui_shared::theme::TEXT_MUTED()),
                );
            });
        });

        ui.add_space(4.0);

        let available = ui.available_size();
        let table_height = available.y - 4.0;

        egui::ScrollArea::vertical()
            .max_height(table_height.max(100.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("events_grid")
                    .num_columns(4)
                    .min_col_width(60.0)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("时间")
                                .strong()
                                .color(opcuaegui_shared::theme::TEXT_PRIMARY()),
                        );
                        ui.label(
                            egui::RichText::new("严重性")
                                .strong()
                                .color(opcuaegui_shared::theme::TEXT_PRIMARY()),
                        );
                        ui.label(
                            egui::RichText::new("来源")
                                .strong()
                                .color(opcuaegui_shared::theme::TEXT_PRIMARY()),
                        );
                        ui.label(
                            egui::RichText::new("消息")
                                .strong()
                                .color(opcuaegui_shared::theme::TEXT_PRIMARY()),
                        );
                        ui.end_row();

                        for item in state.items.iter().rev() {
                            let time_short = format_time_short(&item.time);
                            ui.label(
                                egui::RichText::new(time_short)
                                    .small()
                                    .color(opcuaegui_shared::theme::TEXT_MUTED())
                                    .monospace(),
                            );
                            let sev_color = severity_color(item.severity);
                            ui.label(
                                egui::RichText::new(format!("{}", item.severity)).color(sev_color),
                            );
                            ui.label(
                                egui::RichText::new(&item.source)
                                    .small()
                                    .color(opcuaegui_shared::theme::TEXT_PRIMARY()),
                            );
                            ui.label(
                                egui::RichText::new(&item.message)
                                    .color(opcuaegui_shared::theme::TEXT_PRIMARY()),
                            );
                            ui.end_row();
                        }
                    });
            });
    }

    // Dispatch deferred actions now that the `model.events` borrow is released.
    if let Some((conn_id, source_node_id)) = subscribe_now {
        let req_id = model.alloc_req_id();
        model.events.pending_subscribe_req = Some(req_id);
        backend.send(UiCommand::SubscribeEventInFlight {
            conn_id,
            req_id,
            source_node_id,
        });
    }
    if let Some(conn_id) = unsubscribe_now {
        backend.send(UiCommand::UnsubscribeEvents { conn_id });
    }
    if let Some(conn_id) = clear_now {
        backend.send(UiCommand::ClearEvents { conn_id });
    }
}

fn format_time_short(t: &str) -> String {
    if t.len() >= 19 {
        t[11..19].to_string()
    } else {
        t.to_string()
    }
}

fn severity_color(severity: u16) -> egui::Color32 {
    use opcuaegui_shared::theme;
    if severity <= 100 {
        theme::STATUS_OK()
    } else if severity <= 500 {
        theme::STATUS_WARN()
    } else {
        theme::STATUS_BAD()
    }
}

pub fn apply_event_items(state: &mut EventsPanelState, items: Vec<EventItemDto>) {
    state.items = items;
}

pub fn apply_subscribe_result(state: &mut EventsPanelState, req_id: u64, ok: bool) {
    if state.pending_subscribe_req != Some(req_id) {
        return;
    }
    state.pending_subscribe_req = None;
    state.subscribed = ok;
}
