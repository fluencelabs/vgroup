#![allow(unused)]

use std::fmt::format;

use cursive::event::Event;
use cursive::logger::Record;
use cursive::reexports::log::{log, Level};
use cursive::theme::{BaseColor, Color, Theme};
use cursive::traits::*;
use cursive::view::SizeConstraint;
use cursive::views::{
    Button, DebugView, Dialog, DummyView, EditView, LinearLayout, ListView, Panel, ResizedView,
    SelectView, SliderView, TextArea, TextContent, TextView, ThemedView,
};
use cursive::{Cursive, CursiveRunnable, Printer};
use futures::StreamExt;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

// use cursive_core::traits::Resizable;

fn worker_item(path: &str, percent: usize) -> LinearLayout {
    let mut path = TextView::new_with_content(TextContent::new(format!("{path}  ")));

    let mut percent_content = TextContent::new(format!("  {percent}%  "));
    let mut percent_label = TextView::new_with_content(percent_content.clone());

    let mut slider = SliderView::horizontal(100)
        .on_change(move |s, percent| percent_content.set_content(format!("  {percent}%  ")));

    slider.set_value(percent);
    let slider = slider.max_width(20);

    let layout = LinearLayout::horizontal()
        .child(path)
        .child(slider)
        .child(percent_label);

    layout
}

#[derive(Debug, Clone)]
pub enum ControlEvent {
    WorkersCreated(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum CGroupRef {
    Tokio,
    Worker(usize),
}

#[derive(Debug, Clone)]
pub enum UIEvent {
    CPULimitChanged(CGroupRef, usize),
}

pub struct UIRef {
    sender: UnboundedSender<ControlEvent>,
    receiver: UnboundedReceiver<UIEvent>,
}

pub struct ControlRef {
    sender: UnboundedSender<UIEvent>,
    receiver: UnboundedReceiver<ControlEvent>,
}

pub struct UIRuntime {
    ui_ref: UIRef,
    control_ref: ControlRef,
    runtime: Runtime,
}

pub fn runtime() -> UIRuntime {
    use tokio::runtime::Builder;

    let (to_ui, from_control) = tokio::sync::mpsc::unbounded_channel();
    let (to_control, from_ui) = tokio::sync::mpsc::unbounded_channel();
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .build()
        .expect("build UI tokio runtime");

    // runtime.spawn(async move {
    //     if let Ok(command) = from_control.try_recv() {
    //
    //     }
    // });

    UIRuntime {
        ui_ref: UIRef {
            sender: to_ui,
            receiver: from_ui,
        },
        control_ref: ControlRef {
            sender: to_control,
            receiver: from_control,
        },
        runtime,
    }
}

pub fn show() {
    let mut siv = cursive::default();
    siv.set_theme(Theme::terminal_default());

    let mut left_select = ListView::new();
    left_select.add_child("", worker_item("nox/tokio/worker_1", 10));
    left_select.add_child("", worker_item("nox/tokio/worker_1", 25));
    left_select.add_child("", worker_item("nox/tokio/worker_2", 76));
    let left_select = Panel::new(left_select);

    let mut left_text = TextContent::new("left begins:\n");
    let mut left = TextView::new_with_content(left_text.clone())
        .full_height()
        .full_width();
    for i in 1..10 {
        left_text.append(format!("left: {}\n", i));
    }
    let left = Panel::new(left);

    let mut right_text = TextContent::new("right begins:\n");
    let mut right = TextView::new_with_content(right_text.clone())
        .full_height()
        .full_width();
    for i in 1..10 {
        right_text.append(format!("right: {}\n", i));
    }
    let right = Panel::new(right);

    let linear_layout = LinearLayout::horizontal()
        .child(left_select)
        .child(left)
        .child(right);

    siv.add_layer(linear_layout);

    siv.run();
}

fn debug_view() {
    // let mut debug = DebugView::new().full_height();
    // debug.set_width(SizeConstraint::Fixed(siv.screen_size().x / 2));
    //
    // cursive::logger::init();
    // log!(Level::Info, "Hello!");
}

fn add_name(s: &mut Cursive) {
    fn ok(s: &mut Cursive, name: &str) {
        s.call_on_name("select", |view: &mut SelectView<String>| {
            view.add_item_str(name)
        });
        s.pop_layer();
    }

    s.add_layer(
        Dialog::around(
            EditView::new()
                .on_submit(ok)
                .with_name("name")
                .fixed_width(10),
        )
        .title("Enter a new name")
        .button("Ok", |s| {
            let name = s
                .call_on_name("name", |view: &mut EditView| view.get_content())
                .unwrap();
            ok(s, &name);
        })
        .button("Cancel", |s| {
            s.pop_layer();
        }),
    );
}

fn delete_name(s: &mut Cursive) {
    let mut select = s.find_name::<SelectView<String>>("select").unwrap();
    match select.selected_id() {
        None => s.add_layer(Dialog::info("No name to remove")),
        Some(focus) => {
            select.remove_item(focus);
        }
    }
}

fn on_submit(s: &mut Cursive, name: &str) {
    s.pop_layer();
    s.add_layer(
        Dialog::text(format!("Name: {}\nAwesome: yes", name))
            .title(format!("{}'s info", name))
            .button("Quit", Cursive::quit),
    );
}
