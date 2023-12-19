use cgroups_rs::{Cgroup, CgroupPid, error};
use cgroups_rs::cgroup::CGROUP_MODE_THREADED;

use crate::{assign_threads, print_group, WORKERS};
use crate::threads::Threads;

#[derive(Debug, Clone)]
pub struct CGroups {
    pub nox: Cgroup,
    pub tokio: Cgroup,
    pub workers: Vec<Cgroup>,
}

impl CGroups {
    pub fn new(workers: usize, threads_num: usize) -> Self {
        // Create nox/tokio cgroup hierarchy
        let cgroups = make_cgroup(workers);

        // Move current PID to the nox group
        // let pid = libc::pid_t::from(nix::unistd::getpid()) as u64;
        // cgroups.nox.add_task_by_tgid(CgroupPid::from(pid)).expect("add pid to nox cgroup");
        //
        // let mut procs = cgroups.nox.procs().into_iter();
        // // Verify that the task is indeed in the x control group
        // assert_eq!(procs.next(), Some(CgroupPid::from(pid)));
        // assert_eq!(procs.next(), None);

        // Retrieve 'nox' cgroup, it's the parent cgroup of the tokio cgroup
        println!("\n# nox:");
        print_group(&cgroups.nox);

        // Print tokio group before and after cpu limit
        println!("# tokio:");
        print_group(&cgroups.tokio);

        // create tokio runtime with N threads
        // let threads_num = read_usize("Threads");
        let threads = Threads::new(threads_num);
        println!("created threads {:?}", threads.ids);

        let group_size = threads_num / WORKERS;
        let thread_ids = threads.ids.chunks(group_size);
        for (i, ids) in thread_ids.enumerate() {
            // Move groups of threads to 'tokio/worker_$i' cgroup
            assign_threads(&cgroups.workers[i], ids);
        }

        cgroups
    }
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
