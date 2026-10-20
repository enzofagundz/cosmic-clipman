// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::Local;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{Limits, Subscription, futures, window::Id};
use cosmic::prelude::*;
use cosmic::widget;
use futures::{SinkExt, StreamExt};

use crate::dbus::{self, IpcRequest};
use crate::domain::{self, Item, RETENTION_LIMIT, SENSITIVE_TIMEOUT_SECS};
use crate::fl;
use crate::inject;
use crate::sensitive;
use crate::store::Store;
use crate::watcher::{self, ClipEvent, Shared};

const POPUP_MAX_HEIGHT: f32 = 640.0;
const POPUP_LIST_HEIGHT: f32 = 380.0;
const POPUP_MIN_WIDTH: f32 = 380.0;
const PASTE_DELAY: Duration = Duration::from_millis(160);

pub struct App {
    core: cosmic::Core,
    popup: Option<Id>,
    store: Option<Store>,
    items: Vec<Item>,
    query: String,
    search_id: widget::Id,
    selected: usize,
    incognito: bool,
    shared: Arc<Shared>,
    status: Option<String>,
    confirm_clear: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Clip(ClipEvent),
    Capturing(bool),
    Query(String),
    Key(cosmic::iced::keyboard::Event),
    Ipc(IpcRequest),
    Activate(Option<i64>),
    Copy(Option<i64>),
    TogglePin(Option<i64>),
    Delete(Option<i64>),
    ClearRequest,
    ClearConfirm,
    ClearCancel,
    Incognito(bool),
    PasteResult(Result<(), String>),
    Tick,
}

impl App {
    fn now() -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs_f64())
            .unwrap_or_default()
    }

    fn reload(&mut self) {
        if let Some(store) = &self.store {
            match store.list(RETENTION_LIMIT) {
                Ok(items) => self.items = items,
                Err(err) => self.status = Some(err.to_string()),
            }
        }
        let count = self.visible().len();
        if count == 0 {
            self.selected = 0;
        } else if self.selected >= count {
            self.selected = count - 1;
        }
    }

    /// Items matching the current search, in display order.
    fn visible(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|item| item.matches(&self.query))
            .collect()
    }

    fn selected_id(&self) -> Option<i64> {
        self.visible().get(self.selected).map(|item| item.id)
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.visible().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, count as isize - 1) as usize;
    }

    fn item(&self, id: i64) -> Option<Item> {
        self.items.iter().find(|item| item.id == id).cloned()
    }

    fn open_popup(&mut self) -> Task<cosmic::Action<Message>> {
        if self.popup.is_some() {
            return Task::none();
        }
        let Some(main_id) = self.core.main_window_id() else {
            return Task::none();
        };
        let new_id = Id::unique();
        self.popup = Some(new_id);
        let mut settings = self
            .core
            .applet
            .get_popup_settings(main_id, new_id, None, None, None);
        settings.positioner.size_limits = Limits::NONE
            .max_width(520.0)
            .min_width(POPUP_MIN_WIDTH)
            .min_height(360.0)
            .max_height(POPUP_MAX_HEIGHT);
        Task::batch(vec![
            get_popup(settings),
            widget::text_input::focus(self.search_id.clone()),
        ])
    }

    fn close_popup(&mut self) -> Task<cosmic::Action<Message>> {
        match self.popup.take() {
            Some(id) => destroy_popup(id),
            None => Task::none(),
        }
    }

    fn toggle_popup(&mut self) -> Task<cosmic::Action<Message>> {
        if self.popup.is_some() {
            self.close_popup()
        } else {
            self.open_popup()
        }
    }

    /// Show the popup, refreshing the list first, so a shortcut always lands on
    /// current data.
    fn show_popup(&mut self) -> Task<cosmic::Action<Message>> {
        self.reload();
        self.open_popup()
    }

    fn copy_task(&mut self, id: i64, then_paste: bool) -> Task<cosmic::Action<Message>> {
        let Some(item) = self.item(id) else {
            return Task::none();
        };
        if let Some(store) = &mut self.store
            && let Err(err) = store.touch(id, Self::now())
        {
            self.status = Some(err.to_string());
        }
        self.shared.arm_self_copy(Instant::now());
        self.reload();
        cosmic::task::future(async move {
            let result = tokio::task::spawn_blocking(move || {
                watcher::set_text(&item.text)?;
                if then_paste {
                    std::thread::sleep(PASTE_DELAY);
                    inject::press_ctrl_v().map_err(|err| err.to_string())?;
                }
                Ok::<(), String>(())
            })
            .await
            .unwrap_or_else(|err| Err(format!("paste worker failed: {err}")));
            Message::PasteResult(result)
        })
    }

    fn delete(&mut self, id: i64) {
        if let Some(store) = &mut self.store
            && let Err(err) = store.delete(id)
        {
            self.status = Some(err.to_string());
        }
        self.reload();
    }

    fn toggle_pin(&mut self, id: i64) {
        let Some(item) = self.item(id) else {
            return;
        };
        if let Some(store) = &mut self.store
            && let Err(err) = store.set_pin(id, !item.pinned)
        {
            self.status = Some(err.to_string());
        }
        self.reload();
    }

    fn purge_sensitive(&mut self) {
        let Some(store) = &mut self.store else {
            return;
        };
        match store.purge_expired_sensitive(Self::now(), SENSITIVE_TIMEOUT_SECS) {
            Ok(0) => {}
            Ok(_) => self.reload(),
            Err(err) => self.status = Some(err.to_string()),
        }
    }

    fn handle_key(
        &mut self,
        event: cosmic::iced::keyboard::Event,
    ) -> Task<cosmic::Action<Message>> {
        use cosmic::iced::keyboard::Key;
        use cosmic::iced::keyboard::key::Named;

        let cosmic::iced::keyboard::Event::KeyPressed { key, .. } = event else {
            return Task::none();
        };
        match key {
            Key::Named(Named::Escape) => self.close_popup(),
            Key::Named(Named::ArrowDown) => {
                self.move_selection(1);
                Task::none()
            }
            Key::Named(Named::ArrowUp) => {
                self.move_selection(-1);
                Task::none()
            }
            Key::Named(Named::Enter) => {
                let Some(id) = self.selected_id() else {
                    return Task::none();
                };
                let task = self.copy_task(id, true);
                Task::batch(vec![self.close_popup(), task])
            }
            Key::Named(Named::Delete) => {
                if let Some(id) = self.selected_id() {
                    self.delete(id);
                }
                Task::none()
            }
            Key::Character(character) if character.eq_ignore_ascii_case("p") => {
                if let Some(id) = self.selected_id() {
                    self.toggle_pin(id);
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn row_meta(item: &Item) -> String {
        chrono::DateTime::from_timestamp(item.accessed_at as i64, 0)
            .map(|stamp| stamp.with_timezone(&Local).format("%H:%M").to_string())
            .unwrap_or_default()
    }
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "io.github.enzofagundz.CosmicClipman";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let (store, status) = match Store::open_default() {
            Ok(store) => (Some(store), None),
            Err(err) => (
                Store::open_in_memory().ok(),
                Some(format!("history unavailable: {err}")),
            ),
        };
        let mut app = App {
            core,
            popup: None,
            store,
            items: Vec::new(),
            query: String::new(),
            search_id: widget::Id::new("clipman-search"),
            selected: 0,
            incognito: false,
            shared: Arc::new(Shared::default()),
            status,
            confirm_clear: false,
        };
        app.reload();
        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("edit-paste-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let visible = self.visible();
        let selected_id = visible.get(self.selected).map(|item| item.id);
        let mut list = widget::list_column();
        if visible.is_empty() {
            let message = if self.query.trim().is_empty() {
                fl!("empty")
            } else {
                fl!("no-results")
            };
            list = list.add(widget::text::body(message));
        } else {
            let mut last_group: Option<domain::Group> = None;
            for (group, rows) in domain::grouped(&visible, Local::now()) {
                if last_group != Some(group) {
                    list = list.add(widget::text::heading(group.label()));
                    last_group = Some(group);
                }
                for item in rows {
                    let row = widget::row(vec![
                        widget::text::body(item.preview()).into(),
                        widget::text::caption(Self::row_meta(item)).into(),
                    ])
                    .spacing(8);
                    list = list.add(
                        widget::list::button(row)
                            .on_press(Message::Activate(Some(item.id)))
                            .selected(selected_id == Some(item.id)),
                    );
                }
            }
        }

        let mut content = widget::column(vec![
            widget::text_input::search_input(fl!("search-placeholder"), &self.query)
                .id(self.search_id.clone())
                .on_input(Message::Query)
                .into(),
        ])
        .spacing(8);

        if let Some(status) = &self.status {
            content = content.push(widget::text::caption(status.clone()));
        }

        if self.confirm_clear {
            content = content.push(widget::text::body(fl!("clear-confirm")));
            content = content.push(widget::text::caption(fl!("clear-confirm-body")));
            content = content.push(
                widget::row(vec![
                    widget::button::destructive(fl!("clear"))
                        .on_press(Message::ClearConfirm)
                        .into(),
                    widget::button::standard(fl!("cancel"))
                        .on_press(Message::ClearCancel)
                        .into(),
                ])
                .spacing(8),
            );
        }

        content = content
            .push(widget::scrollable(list).height(cosmic::iced::Length::Fixed(POPUP_LIST_HEIGHT)));

        let mut footer_items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(id) = selected_id {
            let pinned = self.item(id).is_some_and(|item| item.pinned);
            footer_items.push(
                widget::button::standard(if pinned { fl!("unpin") } else { fl!("pin") })
                    .on_press(Message::TogglePin(Some(id)))
                    .into(),
            );
            footer_items.push(
                widget::button::standard(fl!("copy"))
                    .on_press(Message::Copy(Some(id)))
                    .into(),
            );
            footer_items.push(
                widget::button::standard(fl!("delete"))
                    .on_press(Message::Delete(Some(id)))
                    .into(),
            );
        }
        footer_items.push(widget::text::body(fl!("incognito")).into());
        footer_items.push(
            widget::toggler(self.incognito)
                .on_toggle(Message::Incognito)
                .into(),
        );
        footer_items.push(
            widget::button::destructive(fl!("clear"))
                .on_press(Message::ClearRequest)
                .into(),
        );
        content = content.push(
            widget::row(footer_items)
                .spacing(12)
                .align_y(cosmic::iced::Alignment::Center),
        );

        self.core.applet.popup_container(content).into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            service_subscription(self.shared.clone()),
            cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick),
            cosmic::iced::keyboard::listen().map(Message::Key),
        ])
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::TogglePopup => return self.toggle_popup(),
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Capturing(owned) => {
                if !owned {
                    self.status = Some("another applet instance is recording".into());
                }
            }
            Message::Clip(ClipEvent::Text(text)) => {
                if self.shared.incognito() {
                    return Task::none();
                }
                let sensitive = sensitive::is_sensitive(&text);
                if let Some(store) = &mut self.store
                    && let Err(err) = store.insert_text(&text, sensitive, Self::now())
                {
                    self.status = Some(err.to_string());
                }
                self.reload();
            }
            Message::Clip(ClipEvent::Failed(err)) => self.status = Some(err),
            Message::Query(query) => {
                self.query = query;
                self.selected = 0;
            }
            Message::Key(event) => return self.handle_key(event),
            Message::Ipc(IpcRequest::Toggle) => return self.toggle_popup(),
            Message::Ipc(IpcRequest::Show) => return self.show_popup(),
            Message::Ipc(IpcRequest::Hide) => return self.close_popup(),
            Message::Activate(Some(id)) => {
                let task = self.copy_task(id, true);
                return Task::batch(vec![self.close_popup(), task]);
            }
            Message::Copy(Some(id)) => return self.copy_task(id, false),
            Message::Activate(None) | Message::Copy(None) => {}
            Message::TogglePin(Some(id)) => self.toggle_pin(id),
            Message::TogglePin(None) => {}
            Message::Delete(Some(id)) => self.delete(id),
            Message::Delete(None) => {}
            Message::ClearRequest => self.confirm_clear = true,
            Message::ClearCancel => self.confirm_clear = false,
            Message::ClearConfirm => {
                self.confirm_clear = false;
                if let Some(store) = &mut self.store
                    && let Err(err) = store.clear_unpinned()
                {
                    self.status = Some(err.to_string());
                }
                self.reload();
            }
            Message::Incognito(on) => {
                self.incognito = on;
                self.shared.set_incognito(on);
            }
            Message::PasteResult(Err(err)) => self.status = Some(err),
            Message::PasteResult(Ok(())) => self.status = None,
            Message::Tick => self.purge_sensitive(),
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

fn service_subscription(shared: Arc<Shared>) -> Subscription<Message> {
    Subscription::run_with(ServiceData { shared }, service_stream)
}

struct ServiceData {
    shared: Arc<Shared>,
}

impl std::hash::Hash for ServiceData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        0u8.hash(state);
    }
}

fn service_stream(
    data: &ServiceData,
) -> std::pin::Pin<Box<dyn futures::Stream<Item = Message> + Send>> {
    let shared = data.shared.clone();
    Box::pin(cosmic::iced::stream::channel(
        16,
        move |mut output: futures::channel::mpsc::Sender<Message>| async move {
            match dbus::serve().await {
                Ok(mut running) => {
                    let _ = output.send(Message::Capturing(running.owned)).await;
                    if !running.owned {
                        while let Some(request) = running.requests.recv().await {
                            if output.send(Message::Ipc(request)).await.is_err() {
                                return;
                            }
                        }
                        return;
                    }
                    let (sender, mut events) = futures::channel::mpsc::unbounded();
                    watcher::spawn(shared, sender);
                    loop {
                        tokio::select! {
                            event = events.next() => match event {
                                Some(event) => {
                                    if output.send(Message::Clip(event)).await.is_err() {
                                        return;
                                    }
                                }
                                None => return,
                            },
                            request = running.requests.recv() => match request {
                                Some(request) => {
                                    if output.send(Message::Ipc(request)).await.is_err() {
                                        return;
                                    }
                                }
                                None => return,
                            },
                        }
                    }
                }
                Err(err) => {
                    let _ = output.send(Message::Capturing(false)).await;
                    let _ = output
                        .send(Message::Clip(ClipEvent::Failed(format!(
                            "session bus unavailable: {err}"
                        ))))
                        .await;
                }
            }
        },
    ))
}
