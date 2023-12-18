extern crate core;

use std::io::Write;
use std::ops::{Div, Mul};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cgroups_rs::cgroup::CGROUP_MODE_THREADED;
#[allow(unused)]
use cgroups_rs::cgroup_builder::*;
use cgroups_rs::cpu::CpuController;
#[allow(unused)]
use cgroups_rs::*;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use tokio::runtime::Runtime;
use tokio::task::yield_now;

mod ui;

const BACKSPACE: char = 8u8 as char;
const WORKERS: usize = 3;

// #[tokio::main]
fn main() {
    ui::show();

    // Create nox/tokio cgroup hierarchy
    let cgroups = make_cgroup(WORKERS);

    // Retrieve 'nox' cgroup, it's the parent cgroup of the tokio cgroup
    println!("\n# nox:");
    print_group(&cgroups.nox);

    // Print tokio group before and after cpu limit
    println!("# tokio:");
    print_group(&cgroups.tokio);

    // Move current PID to the nox group
    let pid = libc::pid_t::from(nix::unistd::getpid()) as u64;
    cgroups.nox.add_task_by_tgid(CgroupPid::from(pid)).unwrap();

    let mut procs = cgroups.nox.procs().into_iter();
    // Verify that the task is indeed in the x control group
    assert_eq!(procs.next(), Some(CgroupPid::from(pid)));
    assert_eq!(procs.next(), None);

    // create tokio runtime with N threads
    let threads_num = read_threads();
    let threads = create_threads(threads_num);
    println!("created threads {:?}", threads.ids);

    let group_size = threads_num / WORKERS;
    let thread_ids = threads.ids.chunks(group_size);
    for (i, ids) in thread_ids.enumerate() {
        // Move groups of threads to 'tokio/worker_$i' cgroup
        assign_threads(&cgroups.workers[i], ids);
    }

    read_limit(threads, cgroups);
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
            "ui" => ui::show(),
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

fn read_threads() -> usize {
    print!("Threads: _{BACKSPACE}");
    std::io::stdout().flush().expect("flush");

    let mut threads = String::new();
    std::io::stdin().read_line(&mut threads).expect("read");
    let threads = threads.strip_suffix("\n").expect("strip");
    let threads = threads.parse().expect("parse");

    threads
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

struct Threads {
    stop: Arc<AtomicBool>,
    join: BoxFuture<'static, ()>,
    ids: Vec<u64>,
    runtime: Runtime,
}

async fn thread_body(stop: Arc<AtomicBool>) {
    yield_now().await;
    loop {
        if stop.load(Ordering::SeqCst) {
            println!("thread stopped: {}", get_tid());
            std::io::stdout().flush().expect("flush");
            break;
        }

        let floats: Vec<f64> = (1..1000000).map(|n| 1f64 / n as f64).collect::<Vec<_>>();
        let sum: f64 = floats.clone().into_iter().sum();
        let floats = floats
            .clone()
            .into_iter()
            .map(|f| {
                let exp = f.exp();
                let exp2 = f.exp2();

                sum.div(exp) - exp2.div(sum)
            })
            .collect::<Vec<_>>();
        let sum: f64 = floats.into_iter().sum::<f64>() + sum;
        if sum < 0f64 {
            println!("DID NOT EXPECT THAT");
        }
    }
}

fn create_threads(n: usize) -> Threads {
    use tokio::runtime::Builder;

    // let (sender, receiver) = tokio::sync::mpsc::channel(n);
    let (sender, receiver) = std::sync::mpsc::channel();
    let runtime = Builder::new_multi_thread()
        .worker_threads(n)
        .max_blocking_threads(1)
        .on_thread_start(move || {
            let tid = get_tid();
            println!("thread started: {}", tid);
            sender.send(tid).expect("send");
        })
        .build()
        .expect("build tokio runtime");

    let stop = Arc::new(AtomicBool::new(false));
    let join = (0..n)
        .map(|_| runtime.spawn(thread_body(stop.clone())))
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .map(|_| ())
        .boxed();

    // TODO: it's possible it will hang here without any way to notify user
    let thread_ids = receiver.into_iter().take(n).collect::<Vec<_>>();

    Threads {
        stop,
        join,
        ids: thread_ids,
        runtime,
    }
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

struct CGroups {
    nox: Cgroup,
    tokio: Cgroup,
    workers: Vec<Cgroup>,
}

fn make_cgroup(workers: usize) -> CGroups {
    use cgroups_rs::hierarchies::auto;

    let nox = Cgroup::new(auto(), String::from("nox")).unwrap();

    let tokio = Cgroup::new_with_specified_controllers(
        auto(),
        String::from("nox/tokio"),
        Some(vec![String::from("cpuset"), String::from("cpu")]),
    )
    .expect("create tokio cg");

    let workers = (0..workers)
        .map(|i| {
            let path = format!("nox/tokio/worker_{}", i);
            let controllers = vec!["cpuset", "cpu"]
                .into_iter()
                .map(|g| g.to_string())
                .collect();
            let group = Cgroup::new_with_specified_controllers(auto(), path, Some(controllers))?;
            // Set cgroup type of the sub-control group is thread mode.
            group.set_cgroup_type(CGROUP_MODE_THREADED).unwrap();
            Ok(group)
        })
        .collect::<error::Result<_>>()
        .expect("create worker cgroups");

    CGroups {
        nox,
        tokio,
        workers,
    }
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
