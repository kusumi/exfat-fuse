#[must_use]
pub(crate) fn get_basename(f: &str) -> String {
    std::path::Path::new(&f)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

pub(crate) fn is_dir(f: &str) -> bool {
    if let Ok(v) = std::fs::metadata(f) {
        v.file_type().is_dir()
    } else {
        false
    }
}

pub(crate) fn join_path(f1: &str, f2: &str) -> String {
    std::path::Path::new(f1)
        .join(f2)
        .as_path()
        .to_str()
        .unwrap()
        .to_string()
}

pub(crate) fn get_home_path() -> String {
    home::home_dir()
        .unwrap()
        .into_os_string()
        .into_string()
        .unwrap()
}

pub(crate) fn stat2attr(st: &libexfat::exfat::ExfatStat) -> fuser::FileAttr {
    let mtime = unix2system(st.st_mtime);
    fuser::FileAttr {
        ino: st.st_ino,
        size: st.st_size,
        blocks: st.st_blocks,
        atime: unix2system(st.st_atime),
        mtime,
        ctime: mtime,
        crtime: mtime,
        kind: mode2kind(st.st_mode),
        perm: (st.st_mode & 0o777).try_into().unwrap(),
        nlink: st.st_nlink,
        uid: st.st_uid,
        gid: st.st_gid,
        rdev: st.st_rdev,
        blksize: st.st_blksize,
        flags: 0,
    }
}

pub(crate) fn mode2kind(mode: libexfat::exfat::ExfatStatMode) -> fuser::FileType {
    if (mode & libc::S_IFDIR) != 0 {
        fuser::FileType::Directory
    } else if (mode & libc::S_IFREG) != 0 {
        fuser::FileType::RegularFile
    } else {
        panic!("{mode:o}");
    }
}

pub(crate) fn unix2system(t: u64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(t)
}

const DEBUG: &str = "DEBUG";

pub(crate) fn get_debug_level() -> i32 {
    match std::env::var(DEBUG) {
        Ok(v) => v.parse().unwrap_or(-1),
        Err(_) => -1,
    }
}

pub(crate) fn is_debug_set() -> bool {
    get_debug_level() > 0
}
