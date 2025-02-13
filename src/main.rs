#[macro_use]
extern crate lazy_static;

mod fuse;
mod util;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn print_version() {
    println!("Copyright (C) 2011-2023  Andrew Nayenko");
    println!("Copyright (C) 2024-  Tomohiro Kusumi");
}

const EXFAT_HOME: &str = "EXFAT_HOME";
const EXFAT_NIDALLOC: &str = "EXFAT_NIDALLOC";

struct ExfatFuse {
    ef: libexfat::exfat::Exfat,
    total_open: usize,
    debug: i32,
}

impl ExfatFuse {
    fn new(ef: libexfat::exfat::Exfat, debug: i32) -> Self {
        Self {
            ef,
            total_open: 0,
            debug,
        }
    }
}

fn init_std_logger() -> std::result::Result<(), log::SetLoggerError> {
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

fn init_file_logger(prog: &str) -> Result<()> {
    let dir = util::get_home_path()?;
    let name = format!(
        ".{}.log",
        match util::get_basename(prog) {
            Some(v) => v,
            None => "exfat-fuse".to_string(),
        }
    );
    let f = match std::env::var(EXFAT_HOME) {
        Ok(v) => {
            if util::is_dir(&v) {
                util::join_path(&v, &name)?
            } else {
                eprintln!("{EXFAT_HOME} not a directory, using {dir} instead");
                util::join_path(&dir, &name)?
            }
        }
        Err(_) => return Err(Box::new(nix::errno::Errno::ENOENT)),
    };
    Ok(simplelog::CombinedLogger::init(vec![
        simplelog::WriteLogger::new(
            if util::is_debug_set() {
                simplelog::LevelFilter::Trace
            } else {
                simplelog::LevelFilter::Info
            },
            simplelog::Config::default(),
            std::fs::File::create(f)?,
        ),
    ])?)
}

fn init_syslog_logger(prog: &str) -> Result<()> {
    let formatter = syslog::Formatter3164 {
        facility: syslog::Facility::LOG_USER,
        hostname: None,
        process: match util::get_basename(prog) {
            Some(v) => v,
            None => "exfat-fuse".to_string(),
        },
        pid: 0,
    };
    let logger = syslog::unix(formatter)?;
    Ok(
        log::set_boxed_logger(Box::new(syslog::BasicLogger::new(logger))).map(|()| {
            log::set_max_level(if util::is_debug_set() {
                //log::LevelFilter::Trace // XXX not traced
                log::LevelFilter::Info
            } else {
                log::LevelFilter::Info
            });
        })?,
    )
}

fn usage(prog: &str, gopt: &getopts::Options) {
    print!(
        "{}",
        gopt.usage(&format!("Usage: {prog} [options] <device> <directory>"))
    );
}

#[allow(clippy::too_many_lines)]
fn main() {
    println!(
        "FUSE exfat {}.{}.{} (fuser)",
        libexfat::VERSION[0],
        libexfat::VERSION[1],
        libexfat::VERSION[2]
    );

    let args: Vec<String> = std::env::args().collect();
    let prog = &args[0];

    let mut gopt = getopts::Options::new();
    // https://docs.rs/fuser/latest/fuser/enum.MountOption.html
    gopt.optflag(
        "",
        "allow_other",
        "Allow all users to access files on this filesystem. \
        By default access is restricted to the user who mounted it.",
    );
    gopt.optflag(
        "",
        "allow_root",
        "Allow the root user to access this filesystem, \
        in addition to the user who mounted it.",
    );
    gopt.optflag("", "ro", "Read-only filesystem");
    gopt.optflag("", "noexec", "Dont allow execution of binaries.");
    gopt.optflag("", "noatime", "Dont update inode access time.");
    if libexfat::util::is_linux() {
        gopt.optflag(
            "",
            "auto_unmount",
            "Automatically unmount when the mounting process exits. \
            AutoUnmount requires AllowOther or AllowRoot. \
            If AutoUnmount is set and neither Allow... is set, \
            the FUSE configuration must permit allow_other, \
            otherwise mounting will fail. \
            Available on Linux.",
        );
        gopt.optflag(
            "",
            "dirsync",
            "All modifications to directories will be done synchronously. \
            Available on Linux.",
        );
        gopt.optflag(
            "",
            "sync",
            "All I/O will be done synchronously. \
            Available on Linux.",
        );
    }
    // options from relan/exfat
    gopt.optopt(
        "",
        "umask",
        "Set the umask (the bitmask of the permissions that are not present, in octal). \
        The default is 0.",
        "<value>",
    );
    gopt.optopt(
        "",
        "dmask",
        "Set the umask for directories only.",
        "<value>",
    );
    gopt.optopt("", "fmask", "Set the umask for files only.", "<value>");
    gopt.optopt(
        "",
        "uid",
        "Set the owner for all files and directories. \
        The default is the owner of the current process.",
        "<value>",
    );
    gopt.optopt(
        "",
        "gid",
        "Set the group for all files and directories. \
        The default is the group of the current process.",
        "<value>",
    );
    gopt.optopt(
        "o",
        "",
        "relan/exfat compatible file system specific options.",
        "<options>",
    );
    gopt.optflag("d", "", "Enable env_logger logging and do not daemonize.");
    // other options
    gopt.optflag("V", "version", "Print version and copyright.");
    gopt.optflag("h", "help", "Print usage.");

    let matches = match gopt.parse(&args[1..]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            usage(prog, &gopt);
            std::process::exit(1);
        }
    };
    if matches.opt_present("V") {
        print_version();
        std::process::exit(0);
    }
    if matches.opt_present("help") {
        usage(prog, &gopt);
        std::process::exit(0);
    }

    let args = &matches.free;
    if args.len() != 2 {
        usage(prog, &gopt);
        std::process::exit(1);
    }
    let spec = &args[0];
    let mntpt = &args[1];

    let mut fopt = vec![
        fuser::MountOption::FSName(spec.clone()),
        fuser::MountOption::Subtype("exfat".to_string()),
        fuser::MountOption::DefaultPermissions,
        #[cfg(target_os = "linux")]
        fuser::MountOption::NoDev,
        fuser::MountOption::NoSuid,
    ];
    let mut mopt = vec![];
    // https://docs.rs/fuser/latest/fuser/enum.MountOption.html
    if matches.opt_present("allow_other") {
        fopt.push(fuser::MountOption::AllowOther);
    }
    if matches.opt_present("allow_root") {
        fopt.push(fuser::MountOption::AllowRoot);
    }
    if matches.opt_present("noexec") {
        fopt.push(fuser::MountOption::NoExec);
    } else {
        fopt.push(fuser::MountOption::Exec);
    }
    if libexfat::util::is_linux() {
        if matches.opt_present("auto_unmount") {
            fopt.push(fuser::MountOption::AutoUnmount);
        }
        if matches.opt_present("dirsync") {
            fopt.push(fuser::MountOption::DirSync);
        }
        if matches.opt_present("sync") {
            fopt.push(fuser::MountOption::Sync);
        }
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
            mopt.extend_from_slice(&[*s, &v[i]]);
        }
    }
    let options = matches.opt_str("o").unwrap_or_default();
    for x in &options.split(',').collect::<Vec<&str>>() {
        let mut found = false;
        let l = x.split('=').collect::<Vec<&str>>();
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
                    mopt.extend_from_slice(&[s, l[1]]);
                    found = true;
                }
            }
        }
        if !found {
            eprintln!("invalid option: {x}");
            std::process::exit(1);
        }
    }
    let use_daemon = !matches.opt_present("d"); // not debug

    if util::is_debug_set() {
        mopt.push("--debug");
    }

    let nidalloc = std::env::var(EXFAT_NIDALLOC).unwrap_or_default();
    if !nidalloc.is_empty() {
        mopt.extend_from_slice(&["--nidalloc", &nidalloc]);
    }

    if ro {
        mopt.extend_from_slice(&["--mode", "ro"]);
    } else {
        mopt.extend_from_slice(&["--mode", "any"]);
    }
    if noatime {
        fopt.push(fuser::MountOption::NoAtime);
        mopt.push("--noatime");
    } else {
        fopt.push(fuser::MountOption::Atime);
    }

    if !use_daemon {
        if let Err(e) = init_std_logger() {
            eprintln!("{e}");
            std::process::exit(1);
        }
    } else if init_file_logger(prog).is_err() {
        if let Err(e) = init_syslog_logger(prog) {
            eprintln!("syslog logger: {e}");
        }
    }

    let ef = match libexfat::mount(spec, &mopt) {
        Ok(v) => v,
        Err(e) => {
            log::error!("{e}");
            if use_daemon {
                eprintln!("{e}");
            }
            std::process::exit(1);
        }
    };
    // fuser option unknown until libexfat mount
    if ef.is_readonly() {
        fopt.push(fuser::MountOption::RO);
    } else {
        fopt.push(fuser::MountOption::RW);
    }
    log::debug!("{fopt:?}");

    if use_daemon {
        // https://docs.rs/daemonize/latest/daemonize/struct.Daemonize.html
        if let Err(e) = daemonize::Daemonize::new().start() {
            log::error!("{e}");
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
    // fuser::mount2 doesn't return, hence after daemonize
    // XXX use fuser::spawn_mount2
    if let Err(e) = fuser::mount2(ExfatFuse::new(ef, util::get_debug_level()), mntpt, &fopt) {
        log::error!("{e}");
        std::process::exit(1);
    }
}
