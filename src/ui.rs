#![allow(unused)]

use std::fmt::format;
use std::sync::Arc;
use std::time::Duration;

use cursive::{Cursive, CursiveRunnable, Printer};
use cursive::event::Event;
use cursive::logger::Record;
use cursive::reexports::log::{Level, log};
use cursive::theme::{BaseColor, Color, Theme};
use cursive::traits::*;
use cursive::view::SizeConstraint;
use cursive::views::{
    Button, DebugView, Dialog, DummyView, EditView, LinearLayout, ListView, Panel, ResizedView,
    SelectView, SliderView, TextArea, TextContent, TextView, ThemedView,
};
use futures::executor::block_on;
use futures_util::StreamExt;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{Sender, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;

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
    pub sender: Sender<ControlEvent>,
    pub receiver: ReceiverStream<UIEvent>,
}

pub struct ControlRef {
    pub sender: Sender<UIEvent>,
    pub receiver: ReceiverStream<ControlEvent>, // UnboundedReceiver<ControlEvent>,
}

pub struct UIRuntime {
    pub ui_ref: UIRef,
    pub control_ref: ControlRef,
    pub tokio: Runtime,
}

impl UIRuntime {
    pub fn create() -> Self {
        use tokio::runtime::Builder;

        let (to_ui, from_control) = tokio::sync::mpsc::channel(100);
        let (to_control, from_ui) = tokio::sync::mpsc::channel(100);
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
                receiver: ReceiverStream::new(from_ui),
            },
            control_ref: ControlRef {
                sender: to_control,
                receiver: ReceiverStream::new(from_control),
            },
            tokio: runtime,
        }
    }
}

// on worker created => populate list view, show info about cgroups
// on progress changed => change limit

pub fn show() {
    let runtime = UIRuntime::create();

    let mut siv = cursive::default();
    siv.set_theme(Theme::terminal_default());

    let mut workers_list = Arc::new(ListView::new());
    // left_select.add_child("", worker_item("nox/tokio/worker_1", 10));
    // left_select.add_child("", worker_item("nox/tokio/worker_1", 25));
    // left_select.add_child("", worker_item("nox/tokio/worker_2", 76));
    let workers_panel = Panel::new(workers_list.clone());
    let workers_list = Mutex::new(workers_list);

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
        .child(workers_panel)
        .child(left)
        .child(right);

    siv.add_layer(linear_layout);
    // TUI TODO:
    // send CPULimitChanged to control

    // let join_siv = runtime.tokio.spawn(tokio::task::block_in_place(siv.run()));
    // I HEREBY DECLARE: the single blocking thread is the TUI thread. Why not?
    let join_siv = runtime.tokio.spawn_blocking(move || siv.run());

    let UIRuntime {
        ui_ref,
        control_ref,
        tokio,
    } = runtime;

    let join_eventbus = tokio.spawn(async move {
        // EventBus TODO:
        // read events from control
        //  WorkersCreated => populate list view
        let receive_control = control_ref
            .receiver
            .for_each(|event: ControlEvent| async move {
                match event {
                    ControlEvent::WorkersCreated(workers) => {
                        let mut workers_list = workers_list.lock().await;
                        for worker in workers {
                            workers_list.add_child("", worker_item(&worker, 50));
                        }
                    }
                }
            });

        tokio::select! {
            _ = receive_control => {}
        }
    });

    std::thread::sleep(Duration::from_secs(1));

    tokio.spawn(ui_ref
        .sender
        .send(ControlEvent::WorkersCreated(vec![
            "/nox/tokio/worker_0".into()
        ])));

    std::thread::sleep(Duration::from_secs(1));

    tokio.spawn(ui_ref
        .sender
        .send(ControlEvent::WorkersCreated(vec![
            "/nox/tokio/worker_1".into()
        ]))
    );

    std::thread::sleep(Duration::from_secs(3));

    // Control TODO:
    // move ui_ref to control
    // use ui_ref in control to send WorkersCreated
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
