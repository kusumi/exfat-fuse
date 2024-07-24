#[macro_use]
extern crate lazy_static;

mod fuse;
mod util;

fn print_version() {
    println!("Copyright (C) 2011-2023  Andrew Nayenko");
    println!("Copyright (C) 2024-  Tomohiro Kusumi");
}

const EXFAT_HOME: &str = "EXFAT_HOME";
const EXFAT_NIDALLOC: &str = "EXFAT_NIDALLOC";

struct ExfatFuse {
    ef: libexfat::exfat::Exfat,
    debug: i32,
}

impl ExfatFuse {
    fn new(ef: libexfat::exfat::Exfat, debug: i32) -> Self {
        Self { ef, debug }
    }
}

fn init_std_logger() -> Result<(), log::SetLoggerError> {
    let env = env_logger::Env::default().filter_or(
        "RUST_LOG",
        if util::is_debug_set() {
            "trace"
        } else {
            "info"
        },
    );
    env_logger::try_init_from_env(env)
}

fn init_file_logger(prog: &str) -> Result<(), log::SetLoggerError> {
    let home = util::get_home_path();
    let name = format!(".{}.log", util::get_basename(prog));
    let f = match std::env::var(EXFAT_HOME) {
        Ok(v) => {
            if util::is_dir(&v) {
                util::join_path(&v, &name)
            } else {
                eprintln!("{EXFAT_HOME} not a directory, using {home} instead");
                util::join_path(&home, &name)
            }
        }
        Err(_) => util::join_path(&home, &name),
    };
    simplelog::CombinedLogger::init(vec![simplelog::WriteLogger::new(
        if util::is_debug_set() {
            simplelog::LevelFilter::Trace
        } else {
            simplelog::LevelFilter::Info
        },
        simplelog::Config::default(),
        std::fs::File::create(f).unwrap(),
    )])
}

// https://docs.rs/daemonize/latest/daemonize/struct.Daemonize.html
fn daemonize() -> Result<(), daemonize::Error> {
    daemonize::Daemonize::new().start()
}

fn usage(prog: &str, opts: &getopts::Options) {
    print!(
        "{}",
        opts.usage(&format!("Usage: {prog} [options] <device> <directory>"))
    );
}

fn main() {
    println!(
        "FUSE exfat {}.{}.{} (fuser)",
        libexfat::VERSION[0],
        libexfat::VERSION[1],
        libexfat::VERSION[2]
    );

    let args: Vec<String> = std::env::args().collect();
    let prog = &args[0];

    let mut opts = getopts::Options::new();
    // https://docs.rs/fuser/latest/fuser/enum.MountOption.html
    opts.optflag(
        "",
        "allow_other",
        "Allow all users to access files on this filesystem. \
        By default access is restricted to the user who mounted it.",
    );
    opts.optflag(
        "",
        "allow_root",
        "Allow the root user to access this filesystem, \
        in addition to the user who mounted it.",
    );
    opts.optflag(
        "",
        "auto_unmount",
        "Automatically unmount when the mounting process exits. \
        AutoUnmount requires AllowOther or AllowRoot. \
        If AutoUnmount is set and neither Allow... is set, \
        the FUSE configuration must permit allow_other, otherwise mounting will fail.",
    );
    opts.optflag("", "ro", "Read-only filesystem");
    opts.optflag("", "noexec", "Dont allow execution of binaries.");
    opts.optflag("", "noatime", "Dont update inode access time.");
    opts.optflag(
        "",
        "dirsync",
        "All modifications to directories will be done synchronously.",
    );
    opts.optflag("", "sync", "All I/O will be done synchronously.");
    // options from relan/exfat
    opts.optopt(
        "",
        "umask",
        "Set the umask (the bitmask of the permissions that are not present, in octal). \
        The default is 0.",
        "<value>",
    );
    opts.optopt(
        "",
        "dmask",
        "Set the umask for directories only.",
        "<value>",
    );
    opts.optopt("", "fmask", "Set the umask for files only.", "<value>");
    opts.optopt(
        "",
        "uid",
        "Set the owner for all files and directories. \
        The default is the owner of the current process.",
        "<value>",
    );
    opts.optopt(
        "",
        "gid",
        "Set the group for all files and directories. \
        The default is the group of the current process.",
        "<value>",
    );
    opts.optopt(
        "o",
        "",
        "relan/exfat compatible file system specific options.",
        "<options>",
    );
    opts.optflag("d", "", "Enable env_logger logging and do not daemonize.");
    // other options
    opts.optflag("V", "version", "Print version and copyright.");
    opts.optflag("h", "help", "Print usage.");

    let matches = match opts.parse(&args[1..]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            usage(prog, &opts);
            std::process::exit(1);
        }
    };
    if matches.opt_present("V") {
        print_version();
        std::process::exit(0);
    }
    if matches.opt_present("help") {
        usage(prog, &opts);
        std::process::exit(0);
    }

    let args = &matches.free;
    if args.len() != 2 {
        usage(prog, &opts);
        std::process::exit(1);
    }
    let spec = &args[0];
    let mntpt = &args[1];

    let mut fopts = vec![
        fuser::MountOption::FSName(spec.clone()),
        fuser::MountOption::Subtype("exfat".to_string()),
        fuser::MountOption::DefaultPermissions,
        fuser::MountOption::NoDev,
        fuser::MountOption::NoSuid,
    ];
    let mut mopts = vec![];
    // https://docs.rs/fuser/latest/fuser/enum.MountOption.html
    if matches.opt_present("allow_other") {
        fopts.push(fuser::MountOption::AllowOther);
    }
    if matches.opt_present("allow_root") {
        fopts.push(fuser::MountOption::AllowRoot);
    }
    if matches.opt_present("auto_unmount") {
        fopts.push(fuser::MountOption::AutoUnmount);
    }
    if matches.opt_present("noexec") {
        fopts.push(fuser::MountOption::NoExec);
    } else {
        fopts.push(fuser::MountOption::Exec);
    }
    if matches.opt_present("dirsync") {
        fopts.push(fuser::MountOption::DirSync);
    }
    if matches.opt_present("sync") {
        fopts.push(fuser::MountOption::Sync);
    }
    let mut ro = matches.opt_present("ro");
    let mut noatime = matches.opt_present("noatime");
    // options from relan/exfat
    let k = ["--umask", "--dmask", "--fmask", "--uid", "--gid"];
    let mut v = vec![];
    for s in &k {
        v.push(matches.opt_str(&s[2..]).unwrap_or_default());
    }
    for (i, s) in k.iter().enumerate() {
        if !v[i].is_empty() {
            mopts.extend_from_slice(&[*s, &v[i]]);
        }
    }
    let options = matches.opt_str("o").unwrap_or_default();
    let v: Vec<&str> = options.split(',').collect();
    for x in &v {
        let mut found = false;
        let l: Vec<&str> = x.split('=').collect();
        if l.len() == 1 {
            if l[0] == "ro" {
                ro = true;
                found = true;
            } else if l[0] == "noatime" {
                noatime = true;
                found = true;
            } else if l[0].is_empty() {
                found = true; // ignore
            }
        } else if l.len() == 2 {
            for s in &k {
                if l[0] == &s[2..] {
                    mopts.extend_from_slice(&[s, l[1]]);
                    found = true;
                }
            }
        }
        if !found {
            eprintln!("invalid option: {x}");
            std::process::exit(1);
        }
    }
    let nodaemonize = matches.opt_present("d");

    if util::is_debug_set() {
        mopts.push("--debug");
    }

    let nidalloc = std::env::var(EXFAT_NIDALLOC).unwrap_or_default();
    if !nidalloc.is_empty() {
        mopts.extend_from_slice(&["--nidalloc", &nidalloc]);
    }

    if ro {
        mopts.extend_from_slice(&["--mode", "ro"]);
    } else {
        mopts.extend_from_slice(&["--mode", "any"]);
    }
    if noatime {
        fopts.push(fuser::MountOption::NoAtime);
        mopts.push("--noatime");
    } else {
        fopts.push(fuser::MountOption::Atime);
    }

    if nodaemonize {
        if let Err(e) = init_std_logger() {
            eprintln!("{e}");
            std::process::exit(1);
        }
    } else if true {
        if let Err(e) = init_file_logger(prog) {
            eprintln!("{e}");
            std::process::exit(1);
        }
    } else {
        unreachable!();
    }

    let ef = match libexfat::mount(spec, &mopts) {
        Ok(v) => v,
        Err(e) => {
            log::error!("{e}");
            std::process::exit(1);
        }
    };
    // fuser option unknown until libexfat mount
    if ef.is_readonly() {
        fopts.push(fuser::MountOption::RO);
    } else {
        fopts.push(fuser::MountOption::RW);
    }
    log::debug!("{fopts:?}");

    if !nodaemonize {
        if let Err(e) = daemonize() {
            log::error!("{e}");
            std::process::exit(1);
        }
    }
    if let Err(e) = fuser::mount2(ExfatFuse::new(ef, util::get_debug_level()), mntpt, &fopts) {
        log::error!("{e}");
        std::process::exit(1);
    }
}
