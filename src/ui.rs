#![allow(unused)]

use std::fmt::format;
use std::ops::Deref;
use std::sync::Mutex;
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
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;

use crate::ui::CGroupRef::Worker;
use crate::ui::Names::WorkersList;
use crate::ui::UIEvent::CPULimitChanged;

fn worker_item(path: String, percent: usize, sender: Sender<UIEvent>) -> LinearLayout {
    let mut path_view = TextView::new_with_content(TextContent::new(format!("{path}  ")));

    let mut percent_content = TextContent::new(format!("  {percent}%  "));
    let mut percent_label = TextView::new_with_content(percent_content.clone());

    let mut slider = SliderView::horizontal(100)
        .on_change(move |s, percent| {
            percent_content.set_content(format!("  {percent}%  "));
            let sent = block_on(sender.send(CPULimitChanged(Worker(path.clone()), percent)));
            if let Err(err) = sent {
                panic!("error sending CPULimitChanged event: {}", err);
            }
        });

    slider.set_value(percent);
    let slider = slider.max_width(20);

    let layout = LinearLayout::horizontal()
        .child(path_view)
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
    Worker(String),
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
            .enable_time()
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

#[derive(Debug, Clone)]
pub enum Names {
    WorkersList,
}

impl Into<String> for Names {
    fn into(self) -> String {
        String::from(AsRef::<str>::as_ref(&self))
    }
}

impl AsRef<str> for Names {
    fn as_ref(&self) -> &str {
        match self {
            Names::WorkersList => "workers_list",
        }
    }
}

impl Deref for Names {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        AsRef::<str>::as_ref(self)
    }
}

// on worker created => populate list view, show info about cgroups
// on progress changed => change limit
pub fn make() -> (CursiveRunnable, Runtime, UIRef) {
    let runtime = UIRuntime::create();

    let mut siv = cursive::default();

    siv.set_theme(Theme::terminal_default());

    let mut workers_list = ListView::new().with_name(WorkersList);
    let workers_panel = Panel::new(workers_list).full_width();

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

    let siv_sink = siv.cb_sink().clone();

    let UIRuntime {
        ui_ref,
        control_ref,
        tokio,
    } = runtime;

    tokio.spawn(async move {
        // EventBus TODO:
        // read events from control
        //  WorkersCreated => populate list view
        // println!("EventBus started.");
        let sender = control_ref.sender;
        let receive_control = control_ref.receiver.for_each(move |event: ControlEvent| {
            let sender = sender.clone();
            let siv_sink = siv_sink.clone();
            // println!("EventBus event {event:?}");
            async move {
                match event {
                    ControlEvent::WorkersCreated(workers) => {
                        siv_sink.send(Box::new(move |cursive| add_workers(cursive, workers, sender)));
                    }
                }
            }
        });

        receive_control.await;

        // tokio::select! {
        //     _ = receive_control => {}
        // }
    });

    // Control TODO:
    // move ui_ref to control
    // use ui_ref in control to send WorkersCreated

    (siv, tokio, ui_ref)
}

fn add_workers(cursive: &mut Cursive, worker_paths: Vec<String>, sender: Sender<UIEvent>) {
    let ret = cursive.call_on_name(&WorkersList, |list: &mut ListView| {
        for worker in worker_paths {
            // println!("add worker {worker}");
            list.add_child("", worker_item(worker, 50, sender.clone()));
        }
    });
    assert!(ret.is_some(), "call_on_name failed");
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
