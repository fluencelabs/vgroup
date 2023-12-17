# What is this

This is an attempt to befriend both Cgroups v2 and Tokio Threads.

This project creates a bunch of threads, puts them into 3 groups, and creates a Cgroup for each of them.

Then, it limits CPU utilization by using Threaded CPU Controller from Cgroup v2

# How to run

You will need a Cgroup v2 capable Linux.

Since I use MacOS, I use `multipass` to run Linuxes.

Also, the program would need root privileges to write to `/sys/fs/cgroup`.

```shell
cargo build && sudo "target/debug/vgroup"
```
