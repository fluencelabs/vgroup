extern crate core;

use std::io::Write;
use std::ops::{Div, Mul};
use std::sync::atomic::Ordering;
use std::time::Duration;

use cgroups_rs::*;
use cgroups_rs::cgroup_builder::*;
use cgroups_rs::cpu::CpuController;
use futures::StreamExt;

use crate::cgroups::CGroups;
use crate::threads::Threads;
use crate::ui::{CGroupRef, UIEvent};

mod ui;
mod cgroups;
mod threads;

const BACKSPACE: char = 8u8 as char;
const WORKERS: usize = 3;

fn main() {
    let workers = read_usize("Workers");
    let threads = read_usize("Threads");

    let cgroups = CGroups::new(workers, threads);

    read_usize("Yaay!");

    // let (mut siv, tokio, ui) = ui::make();
    //
    // tokio.spawn(async move {
    //     ui.receiver.for_each(|event| {
    //         let cgroups = cgroups.clone();
    //         async move {
    //             match event {
    //                 UIEvent::CPULimitChanged(CGroupRef::Tokio, limit) => {
    //                     set_cpu_limit(&cgroups.tokio, limit as u64);
    //                     // TODO: send as event
    //                     // print_group(&cgroups.tokio);
    //                 }
    //                 UIEvent::CPULimitChanged(CGroupRef::Worker(path), limit) => {
    //                     if let Some(idx) = path.rsplit("_").take(1).last().map(str::parse::<usize>).transpose().ok().flatten() {
    //                         let group = &cgroups.workers[idx];
    //                         set_cpu_limit(group, limit as u64);
    //                         // TODO: send group info as event
    //                     }
    //                 }
    //             }
    //         }
    //     }).await;
    // });

    // siv.run()
}

fn read_limit(threads: Threads, groups: CGroups) {
    loop {
        print!("<worker idx> <limit %> | tokio <limit %> | stop: _{BACKSPACE}");
        std::io::stdout().flush().expect("flush");

        let mut action = String::new();
        std::io::stdin().read_line(&mut action).expect("read");
        let action = action.strip_suffix("\n").expect("strip");

        match action {
            "stop" => break send_stop(threads),
            _ => {}
        }

        let action = action.split(" ").collect::<Vec<_>>();
        if let &[worker, limit] = action.as_slice() {
            if let Ok(limit) = limit.parse::<u64>() {
                // Set CPU limit for the whole 'nox/tokio' cgroup
                if worker == "tokio" {
                    set_cpu_limit(&groups.tokio, limit);
                    print_group(&groups.tokio);
                    continue;
                }

                // Set CPU limit for a single worker
                if let Ok(worker) = worker.parse::<usize>() {
                    if worker < groups.workers.len() {
                        let group = &groups.workers[worker];
                        set_cpu_limit(group, limit as u64);
                        print_group(group);
                        continue;
                    }
                }
            }
        }

        // None of the 'continues' were hit
        println!("invalid input: {:?}", action);
    }
}

fn send_stop(threads: Threads) {
    println!("will stop.");
    // Set atomic to signal stop
    threads.stop.store(true, Ordering::SeqCst);

    let _ = threads.runtime.block_on(threads.join);
    println!("Done.");
}

fn read_usize(title: &str) -> usize {
    print!("{title}: _{BACKSPACE}");
    std::io::stdout().flush().expect("flush");

    let mut number = String::new();
    std::io::stdin().read_line(&mut number).expect("read");
    let number = number.strip_suffix("\n").expect("strip");
    let number = number.parse().expect("parse");

    number
}

fn assign_threads(group: &Cgroup, thread_ids: &[u64]) {
    // Move these threads to 'tokio' cgroup
    for tid in thread_ids {
        if let Err(err) = group.add_task(CgroupPid::from(*tid)) {
            println!(
                "error add thread {tid} to '{}' cgroup: {:?}",
                group.path(),
                err
            );
        }
    }
    let tasks = group
        .tasks()
        .into_iter()
        .map(|p| p.pid)
        .collect::<Vec<u64>>();
    println!("threads in '{}' cgroup: {:?}", group.path(), tasks);
}

fn get_tid() -> u64 {
    unsafe { libc::syscall(libc::SYS_gettid) as u64 }
}

/// Set cpu controller period to 10 ms by default. Why not?
const PERIOD_MS: u64 = 10;

fn set_cpu_limit(group: &Cgroup, percent: u64) {
    let period = PERIOD_MS;
    let quota = (period as f64 * percent as f64) / 100f64;
    let quota = quota.ceil() as u64;

    let period = Some(Duration::from_millis(period).as_micros() as u64);
    let quota = Some(Duration::from_millis(quota).as_micros() as i64);

    let ctrl: &CpuController = group
        .controller_of()
        .expect("set_cpu_limit: get group controller");
    ctrl.set_cfs_quota_and_period(quota, period)
        .expect(&format!("set CPU quota to {}", ctrl.path().display()));
}

fn print_group(group: &Cgroup) {
    let parent = group.parent_control_group();
    let parent = parent.path();
    let parent = if parent.is_empty() {
        "no parent"
    } else {
        parent
    };

    let tasks = group
        .tasks()
        .into_iter()
        .map(|p| p.pid.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let tasks = if tasks.is_empty() {
        "no tasks"
    } else {
        tasks.as_str()
    };

    println!("Parent: {}", parent);
    println!("Tasks:  {}", tasks);

    if let Some(ctrl) = group.controller_of() {
        print_controller(ctrl);
    }
}

fn print_controller(ctrl: &CpuController) {
    let quota = ctrl.cfs_quota().unwrap();
    let period = ctrl.cfs_period().unwrap();
    let typ = ctrl.get_cgroup_type().unwrap();
    let path = ctrl.path().display();

    println!("Path:   {: <35}Type: {: <10}", path, typ);
    let percent = (quota as f64).div(period as f64).mul(100f64).to_string();
    let percent = format!("{}%", percent);
    let limit = if quota > 0 {
        percent.as_str()
    } else {
        "no limit"
    };
    println!("Period: {: <35}Quota: {}\t{}\n", period, quota, limit);
}
