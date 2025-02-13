#[must_use]
pub fn get_basename(f: &str) -> Option<String> {
    Some(std::path::Path::new(&f).file_name()?.to_str()?.to_string())
}

pub(crate) fn is_dir(f: &str) -> bool {
    if let Ok(v) = std::fs::metadata(f) {
        v.file_type().is_dir()
    } else {
        false
    }
}

pub(crate) fn join_path(f1: &str, f2: &str) -> crate::Result<String> {
    Ok(std::path::Path::new(f1)
        .join(f2)
        .as_path()
        .to_str()
        .ok_or(nix::errno::Errno::ENOENT)?
        .to_string())
}

pub(crate) fn get_home_path() -> crate::Result<String> {
    Ok(home::home_dir()
        .ok_or(nix::errno::Errno::ENOENT)?
        .into_os_string()
        .into_string()
        .unwrap())
}

pub(crate) fn stat2attr(st: &libexfat::exfat::Stat) -> fuser::FileAttr {
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

pub(crate) fn mode2kind(mode: libexfat::exfat::StatMode) -> fuser::FileType {
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

pub(crate) fn error2errno(e: &std::io::Error) -> Option<nix::errno::Errno> {
    Some(match e.kind() {
        std::io::ErrorKind::AddrInUse => nix::errno::Errno::EADDRINUSE,
        std::io::ErrorKind::AddrNotAvailable => nix::errno::Errno::EADDRNOTAVAIL,
        std::io::ErrorKind::AlreadyExists => nix::errno::Errno::EEXIST,
        std::io::ErrorKind::ArgumentListTooLong => nix::errno::Errno::E2BIG,
        std::io::ErrorKind::BrokenPipe => nix::errno::Errno::EPIPE,
        std::io::ErrorKind::ConnectionAborted => nix::errno::Errno::ECONNABORTED,
        std::io::ErrorKind::ConnectionRefused => nix::errno::Errno::ECONNREFUSED,
        std::io::ErrorKind::ConnectionReset => nix::errno::Errno::ECONNRESET,
        //std::io::ErrorKind::CrossesDevices => nix::errno::Errno::EXDEV,
        std::io::ErrorKind::Deadlock => nix::errno::Errno::EDEADLK,
        std::io::ErrorKind::DirectoryNotEmpty => nix::errno::Errno::ENOTEMPTY,
        //std::io::ErrorKind::FilesystemLoop => nix::errno::Errno::ELOOP,
        //std::io::ErrorKind::FilesystemQuotaExceeded => nix::errno::Errno::EDQUOT,
        std::io::ErrorKind::FileTooLarge => nix::errno::Errno::EFBIG,
        std::io::ErrorKind::HostUnreachable => nix::errno::Errno::EHOSTUNREACH,
        //std::io::ErrorKind::InProgress => nix::errno::Errno::EINPROGRESS,
        std::io::ErrorKind::Interrupted => nix::errno::Errno::EINTR,
        std::io::ErrorKind::InvalidInput => nix::errno::Errno::EINVAL,
        std::io::ErrorKind::IsADirectory => nix::errno::Errno::EISDIR,
        std::io::ErrorKind::NetworkDown => nix::errno::Errno::ENETDOWN,
        std::io::ErrorKind::NetworkUnreachable => nix::errno::Errno::ENETUNREACH,
        std::io::ErrorKind::NotADirectory => nix::errno::Errno::ENOTDIR,
        std::io::ErrorKind::NotConnected => nix::errno::Errno::ENOTCONN,
        std::io::ErrorKind::NotFound => nix::errno::Errno::ENOENT,
        std::io::ErrorKind::NotSeekable => nix::errno::Errno::ESPIPE,
        std::io::ErrorKind::OutOfMemory => nix::errno::Errno::ENOMEM,
        std::io::ErrorKind::PermissionDenied => nix::errno::Errno::EACCES,
        std::io::ErrorKind::ReadOnlyFilesystem => nix::errno::Errno::EROFS,
        std::io::ErrorKind::ResourceBusy => nix::errno::Errno::EBUSY,
        std::io::ErrorKind::StaleNetworkFileHandle => nix::errno::Errno::ESTALE,
        std::io::ErrorKind::StorageFull => nix::errno::Errno::ENOSPC,
        std::io::ErrorKind::TimedOut => nix::errno::Errno::ETIMEDOUT,
        std::io::ErrorKind::TooManyLinks => nix::errno::Errno::EMLINK,
        std::io::ErrorKind::Unsupported => nix::errno::Errno::EOPNOTSUPP,
        std::io::ErrorKind::WouldBlock => nix::errno::Errno::EWOULDBLOCK,
        _ => return None,
    })
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
